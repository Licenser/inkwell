use libc::c_int;
use llvm_sys::execution_engine::{LLVMGetExecutionEngineTargetData, LLVMExecutionEngineRef, LLVMRunFunction, LLVMRunFunctionAsMain, LLVMDisposeExecutionEngine, LLVMGetFunctionAddress, LLVMAddModule, LLVMFindFunction, LLVMLinkInMCJIT, LLVMLinkInInterpreter, LLVMRemoveModule, LLVMGenericValueRef, LLVMFreeMachineCodeForFunction, LLVMAddGlobalMapping, LLVMRunStaticConstructors, LLVMRunStaticDestructors};

use module::Module;
use support::LLVMString;
use targets::TargetData;
use values::{AnyValue, AsValueRef, FunctionValue, GenericValue};

use std::error::Error;
use std::rc::Rc;
use std::ops::Deref;
use std::ffi::{CStr, CString};
use std::fmt::{self, Debug, Display, Formatter};
use std::mem::{forget, uninitialized, zeroed, transmute_copy, size_of};

#[derive(Debug, PartialEq, Eq)]
pub enum FunctionLookupError {
    JITNotEnabled,
    FunctionNotFound, // 404!
}

impl Error for FunctionLookupError {
    // This method is deprecated on nighty so it's probably not
    // something we should worry about
    fn description(&self) -> &str {
        self.as_str()
    }

    fn cause(&self) -> Option<&Error> {
        None
    }
}

impl FunctionLookupError {
    fn as_str(&self) -> &str {
        match self {
            FunctionLookupError::JITNotEnabled => "ExecutionEngine does not have JIT functionality enabled",
            FunctionLookupError::FunctionNotFound => "Function not found in ExecutionEngine",
        }
    }
}

impl Display for FunctionLookupError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "FunctionLookupError({})", self.as_str())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RemoveModuleError {
    ModuleNotOwned,
    IncorrectModuleOwner,
    LLVMError(LLVMString),
}

impl Error for RemoveModuleError {
    // This method is deprecated on nighty so it's probably not
    // something we should worry about
    fn description(&self) -> &str {
        self.as_str()
    }

    fn cause(&self) -> Option<&Error> {
        None
    }
}

impl RemoveModuleError {
    fn as_str(&self) -> &str {
        match self {
            RemoveModuleError::ModuleNotOwned => "Module is not owned by an Execution Engine",
            RemoveModuleError::IncorrectModuleOwner => "Module is not owned by this Execution Engine",
            RemoveModuleError::LLVMError(string) => string.to_str().unwrap_or("LLVMError with invalid unicode"),
        }
    }
}

impl Display for RemoveModuleError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "RemoveModuleError({})", self.as_str())
    }
}

/// A reference-counted wrapper around LLVM's execution engine.
///
/// Cloning this object is essentially just a case of copying a couple pointers
/// and incrementing one or two atomics, so this should be quite cheap to create
/// copies. The underlying LLVM object will be automatically deallocated when
/// there are no more references to it.
#[derive(PartialEq, Eq, Debug)]
pub struct ExecutionEngine {
    execution_engine: ExecEngineInner,
    target_data: Option<TargetData>,
    jit_mode: bool,
}

impl ExecutionEngine {
    pub(crate) fn new(execution_engine: Rc<LLVMExecutionEngineRef>, jit_mode: bool) -> ExecutionEngine {
        assert!(!execution_engine.is_null());

        // REVIEW: Will we have to do this for LLVMGetExecutionEngineTargetMachine too?
        let target_data = unsafe {
            LLVMGetExecutionEngineTargetData(*execution_engine)
        };

        ExecutionEngine {
            execution_engine: ExecEngineInner(execution_engine),
            target_data: Some(TargetData::new(target_data)),
            jit_mode: jit_mode,
        }
    }

    pub fn link_in_mc_jit() {
        unsafe {
            LLVMLinkInMCJIT()
        }
    }

    pub fn link_in_interpreter() {
        unsafe {
            LLVMLinkInInterpreter();
        }
    }

    /// Maps the specified value to an address.
    ///
    /// # Example
    /// ```no_run
    /// use inkwell::targets::{InitializationConfig, Target};
    /// use inkwell::context::Context;
    /// use inkwell::OptimizationLevel;
    ///
    /// Target::initialize_native(&InitializationConfig::default()).unwrap();
    ///
    /// extern fn sumf(a: f64, b: f64) -> f64 {
    ///     a + b
    /// }
    ///
    /// let context = Context::create();
    /// let module = context.create_module("test");
    /// let builder = context.create_builder();
    ///
    /// let ft = context.f64_type();
    /// let fnt = ft.fn_type(&[], false);
    ///
    /// let f = module.add_function("test_fn", &fnt, None);
    /// let b = context.append_basic_block(&f, "entry");
    /// builder.position_at_end(&b);
    ///
    /// let extf = module.add_function("sumf", &ft.fn_type(&[ &ft, &ft ], false), None);
    ///
    /// let argf = ft.const_float(64.);
    /// let retv = builder.build_call(&extf, &[ &argf, &argf ], "retv", false).left().unwrap().into_float_value();
    ///
    /// builder.build_return(Some(&retv));
    ///
    /// let mut ee = module.create_jit_execution_engine(OptimizationLevel::None).unwrap();
    /// ee.add_global_mapping(&extf, sumf as usize);
    ///
    /// let result = unsafe { ee.run_function(&f, &[]) }.as_float(&ft);
    ///
    /// assert_eq!(result, 128.);
    /// ```
    pub fn add_global_mapping(&self, value: &AnyValue, addr: usize) {
        unsafe {
            LLVMAddGlobalMapping(*self.execution_engine, value.as_value_ref(), addr as *mut _)
        }
    }

    /// Adds a module to an `ExecutionEngine`.
    ///
    /// The method will be `Ok(())` if the module does not belong to an `ExecutionEngine` already and `Err(())` otherwise.
    ///
    /// ```rust,no_run
    /// use inkwell::targets::{InitializationConfig, Target};
    /// use inkwell::context::Context;
    /// use inkwell::OptimizationLevel;
    ///
    /// Target::initialize_native(&InitializationConfig::default()).unwrap();
    ///
    /// let context = Context::create();
    /// let module = context.create_module("test");
    /// let mut ee = module.create_jit_execution_engine(OptimizationLevel::None).unwrap();
    ///
    /// assert!(ee.add_module(&module).is_err());
    /// ```
    pub fn add_module(&self, module: &Module) -> Result<(), ()> {
        unsafe {
            LLVMAddModule(*self.execution_engine, module.module.get())
        }

        if module.owned_by_ee.borrow().is_some() {
            return Err(());
        }

        *module.owned_by_ee.borrow_mut() = Some(self.clone());

        Ok(())
    }

    pub fn remove_module(&self, module: &Module) -> Result<(), RemoveModuleError> {
        match *module.owned_by_ee.borrow() {
            Some(ref ee) if *ee.execution_engine != *self.execution_engine => return Err(RemoveModuleError::IncorrectModuleOwner),
            None => return Err(RemoveModuleError::ModuleNotOwned),
            _ => ()
        }

        let mut new_module = unsafe { uninitialized() };
        let mut err_string = unsafe { zeroed() };

        let code = unsafe {
            LLVMRemoveModule(*self.execution_engine, module.module.get(), &mut new_module, &mut err_string)
        };

        if code == 1 {
            return Err(RemoveModuleError::LLVMError(LLVMString::new(err_string)));
        }

        module.module.set(new_module);
        *module.owned_by_ee.borrow_mut() = None;

        Ok(())
    }

    /// Try to load a function from the execution engine.
    ///
    /// If a target hasn't already been initialized, spurious "function not
    /// found" errors may be encountered.
    ///
    /// The [`UnsafeFunctionPointer`] trait is designed so only `unsafe extern
    /// "C"` functions can be retrieved via the `get_function()` method. If you
    /// get funny type errors then it's probably because you have specified the
    /// wrong calling convention or forgotten to specify the retrieved function
    /// as `unsafe`.
    ///
    /// # Examples
    ///
    ///
    /// ```rust
    /// # use inkwell::targets::{InitializationConfig, Target};
    /// # use inkwell::context::Context;
    /// # use inkwell::OptimizationLevel;
    /// # Target::initialize_native(&InitializationConfig::default()).unwrap();
    /// let context = Context::create();
    /// let module = context.create_module("test");
    /// let builder = context.create_builder();
    ///
    /// // Set up the function signature
    /// let double = context.f64_type();
    /// let sig = double.fn_type(&[], false);
    ///
    /// // Add the function to our module
    /// let f = module.add_function("test_fn", &sig, None);
    /// let b = context.append_basic_block(&f, "entry");
    /// builder.position_at_end(&b);
    ///
    /// // Insert a return statement
    /// let ret = double.const_float(64.0);
    /// builder.build_return(Some(&ret));
    ///
    /// // create the JIT engine
    /// let mut ee = module.create_jit_execution_engine(OptimizationLevel::None).unwrap();
    ///
    /// // fetch our JIT'd function and execute it
    /// unsafe {
    ///     let test_fn = ee.get_function::<unsafe extern "C" fn() -> f64>("test_fn").unwrap();
    ///     let return_value = test_fn();
    ///     assert_eq!(return_value, 64.0);
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// It is the caller's responsibility to ensure they call the function with
    /// the correct signature and calling convention.
    ///
    /// The `Symbol` wrapper ensures a function won't accidentally outlive the
    /// execution engine it came from, but adding functions after calling this
    /// method *may* invalidate the function pointer.
    ///
    /// [`UnsafeFunctionPointer`]: trait.UnsafeFunctionPointer.html
    pub unsafe fn get_function<F>(&self, fn_name: &str) -> Result<Symbol<F>, FunctionLookupError>
    where F: UnsafeFunctionPointer
    {
        if !self.jit_mode {
            return Err(FunctionLookupError::JITNotEnabled);
        }

        // LLVMGetFunctionAddress segfaults in llvm 5.0 & 6.0 when fn_name doesn't exist. This is a workaround
        // to see if it exists and avoid the segfault when it doesn't
        #[cfg(any(feature = "llvm5-0", feature = "llvm6-0"))]
        self.get_function_value(fn_name)?;

        let c_string = CString::new(fn_name).expect("Conversion to CString failed unexpectedly");

        let address = LLVMGetFunctionAddress(*self.execution_engine, c_string.as_ptr());

        // REVIEW: Can also return 0 if no targets are initialized.
        // One option might be to set a (thread local?) global to true if any at all of the targets have been
        // initialized (maybe we could figure out which config in particular is the trigger)
        // and if not return an "NoTargetsInitialized" error, instead of not found.
        if address == 0 {
            return Err(FunctionLookupError::FunctionNotFound);
        }

        assert_eq!(size_of::<F>(), size_of::<usize>(),
            "The type `F` must have the same size as a function pointer");

        Ok(Symbol {
            _execution_engine: self.execution_engine.clone(),
            inner: transmute_copy(&address),
        })
    }

    // REVIEW: Not sure if an EE's target data can change.. if so we might want to update the value
    // when making this call
    pub fn get_target_data(&self) -> &TargetData {
        self.target_data.as_ref().expect("TargetData should always exist until Drop")
    }

    // REVIEW: Can also find nothing if no targeting is initialized. Maybe best to
    // do have a global flag for anything initialized. Catch is that it must be initialized
    // before EE is created
    pub fn get_function_value(&self, fn_name: &str) -> Result<FunctionValue, FunctionLookupError> {
        if !self.jit_mode {
            return Err(FunctionLookupError::JITNotEnabled);
        }

        let c_string = CString::new(fn_name).expect("Conversion to CString failed unexpectedly");
        let mut function = unsafe { zeroed() };

        let code = unsafe {
            LLVMFindFunction(*self.execution_engine, c_string.as_ptr(), &mut function)
        };

        if code == 0 {
            return FunctionValue::new(function).ok_or(FunctionLookupError::FunctionNotFound)
        };

        Err(FunctionLookupError::FunctionNotFound)
    }

    // TODOC: Marked as unsafe because input function could very well do something unsafe. It's up to the caller
    // to ensure that doesn't happen by defining their function correctly.
    pub unsafe fn run_function(&self, function: &FunctionValue, args: &[&GenericValue]) -> GenericValue {
        let mut args: Vec<LLVMGenericValueRef> = args.iter()
                                                     .map(|val| val.generic_value)
                                                     .collect();

        let value = LLVMRunFunction(*self.execution_engine, function.as_value_ref(), args.len() as u32, args.as_mut_ptr()); // REVIEW: usize to u32 ok??

        GenericValue::new(value)
    }

    // TODOC: Marked as unsafe because input function could very well do something unsafe. It's up to the caller
    // to ensure that doesn't happen by defining their function correctly.
    // SubType: Only for JIT EEs?
    pub unsafe fn run_function_as_main(&self, function: &FunctionValue, args: &[&str]) -> c_int {
        let cstring_args: Vec<CString> = args.iter().map(|&arg| CString::new(arg).expect("Conversion to CString failed unexpectedly")).collect();
        let raw_args: Vec<*const _> = cstring_args.iter().map(|arg| arg.as_ptr()).collect();

        let environment_variables = vec![]; // TODO: Support envp. Likely needs to be null terminated

        LLVMRunFunctionAsMain(*self.execution_engine, function.as_value_ref(), raw_args.len() as u32, raw_args.as_ptr(), environment_variables.as_ptr()) // REVIEW: usize to u32 cast ok??
    }

    pub fn free_fn_machine_code(&self, function: &FunctionValue) {
        unsafe {
            LLVMFreeMachineCodeForFunction(*self.execution_engine, function.as_value_ref())
        }
    }

    pub fn run_static_constructors(&self) {
        unsafe {
            LLVMRunStaticConstructors(*self.execution_engine)
        }
    }

    pub fn run_static_destructors(&self) {
        unsafe {
            LLVMRunStaticDestructors(*self.execution_engine)
        }
    }
}

// Modules owned by the EE will be discarded by the EE so we don't
// want owned modules to drop.
impl Drop for ExecutionEngine {
    fn drop(&mut self) {
        forget(
            self.target_data
                .take()
                .expect("TargetData should always exist until Drop"),
        );
    }
}

impl Clone for ExecutionEngine {
    fn clone(&self) -> ExecutionEngine {
        ExecutionEngine::new(self.execution_engine.0.clone(), self.jit_mode)
    }
}

/// A smart pointer which wraps the `Drop` logic for `LLVMExecutionEngineRef`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExecEngineInner(Rc<LLVMExecutionEngineRef>);

impl Drop for ExecEngineInner {
    fn drop(&mut self) {
        if Rc::strong_count(&self.0) == 1 {
            unsafe {
                LLVMDisposeExecutionEngine(*self.0);
            }
        }
    }
}

impl Deref for ExecEngineInner {
    type Target = LLVMExecutionEngineRef;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// A wrapper around a function pointer which ensures the symbol being pointed
/// to doesn't accidentally outlive its execution engine.
#[derive(Clone)]
pub struct Symbol<F> {
    _execution_engine: ExecEngineInner,
    inner: F,
}

impl<F: UnsafeFunctionPointer> Deref for Symbol<F> {
    type Target = F;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<F> Debug for Symbol<F> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_tuple("Symbol")
            .field(&"<unnamed>")
            .finish()
    }
}

/// Marker trait representing an unsafe function pointer (`unsafe extern "C" fn(A, B, ...) -> Output`).
pub trait UnsafeFunctionPointer: private::Sealed + Copy {}

mod private {
    /// A sealed trait which ensures nobody outside this crate can implement
    /// `UnsafeFunctionPointer`.
    ///
    /// See https://rust-lang-nursery.github.io/api-guidelines/future-proofing.html
    pub trait Sealed {}
}

macro_rules! impl_unsafe_fn {
    ($( $param:ident ),*) => {
        impl<Output, $( $param ),*> private::Sealed for unsafe extern "C" fn($( $param ),*) -> Output {}
        impl<Output, $( $param ),*> UnsafeFunctionPointer for unsafe extern "C" fn($( $param ),*) -> Output {}
    };
}

impl_unsafe_fn!();
impl_unsafe_fn!(A);
impl_unsafe_fn!(A, B);
impl_unsafe_fn!(A, B, C);
impl_unsafe_fn!(A, B, C, D);
impl_unsafe_fn!(A, B, C, D, E);
impl_unsafe_fn!(A, B, C, D, E, F);
impl_unsafe_fn!(A, B, C, D, E, F, G);
impl_unsafe_fn!(A, B, C, D, E, F, G, H);
impl_unsafe_fn!(A, B, C, D, E, F, G, H, I);
impl_unsafe_fn!(A, B, C, D, E, F, G, H, I, J);
impl_unsafe_fn!(A, B, C, D, E, F, G, H, I, J, K);
impl_unsafe_fn!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_unsafe_fn!(A, B, C, D, E, F, G, H, I, J, K, L, M);
