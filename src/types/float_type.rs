use llvm_sys::core::{LLVMConstReal, LLVMConstNull, LLVMHalfType, LLVMFloatType, LLVMDoubleType, LLVMFP128Type, LLVMPPCFP128Type, LLVMConstRealOfStringAndSize, LLVMX86FP80Type};
use llvm_sys::execution_engine::LLVMCreateGenericValueOfFloat;
use llvm_sys::prelude::LLVMTypeRef;

use AddressSpace;
use context::ContextRef;
use support::LLVMString;
use types::traits::AsTypeRef;
use types::{Type, PointerType, FunctionType, BasicType, ArrayType, VectorType};
use values::{FloatValue, GenericValue, PointerValue, IntValue};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct FloatType {
    float_type: Type,
}

impl FloatType {
    pub(crate) fn new(float_type: LLVMTypeRef) -> Self {
        assert!(!float_type.is_null());

        FloatType {
            float_type: Type::new(float_type),
        }
    }

    pub fn fn_type(&self, param_types: &[&BasicType], is_var_args: bool) -> FunctionType {
        self.float_type.fn_type(param_types, is_var_args)
    }

    pub fn array_type(&self, size: u32) -> ArrayType {
        self.float_type.array_type(size)
    }

    pub fn vec_type(&self, size: u32) -> VectorType {
        self.float_type.vec_type(size)
    }

    pub fn const_float(&self, value: f64) -> FloatValue {
        let value = unsafe {
            LLVMConstReal(self.float_type.type_, value)
        };

        FloatValue::new(value)
    }

    // REVIEW: What happens when string is invalid? Nullptr?
    pub fn const_float_from_string(&self, slice: &str) -> FloatValue {
        let value = unsafe {
            LLVMConstRealOfStringAndSize(self.as_type_ref(), slice.as_ptr() as *const i8, slice.len() as u32)
        };

        FloatValue::new(value)
    }

    pub fn const_null_ptr(&self) -> PointerValue {
        self.float_type.const_null_ptr()
    }

    pub fn const_null(&self) -> FloatValue {
        let null = unsafe {
            LLVMConstNull(self.as_type_ref())
        };

        FloatValue::new(null)
    }

    // REVIEW: Always true -> const fn?
    pub fn is_sized(&self) -> bool {
        self.float_type.is_sized()
    }

    pub fn size_of(&self) -> IntValue {
        self.float_type.size_of()
    }

    pub fn get_context(&self) -> ContextRef {
        self.float_type.get_context()
    }

    pub fn ptr_type(&self, address_space: AddressSpace) -> PointerType {
        self.float_type.ptr_type(address_space)
    }

    /// Creates a new `FloatType` which represents sixteen bits (two bytes) for the global context.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use inkwell::context::Context;
    /// use inkwell::types::FloatType;
    ///
    /// let f16_type = FloatType::f16_type();
    ///
    /// assert_eq!(f16_type.get_context(), Context::get_global());
    /// ```
    pub fn f16_type() -> Self {
        let float_type = unsafe {
            LLVMHalfType()
        };

        FloatType::new(float_type)
    }

    /// Creates a new `FloatType` which represents thirty two bits (four bytes) for the global context.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use inkwell::context::Context;
    /// use inkwell::types::FloatType;
    ///
    /// let f32_type = FloatType::f32_type();
    ///
    /// assert_eq!(f32_type.get_context(), Context::get_global());
    /// ```
    pub fn f32_type() -> Self {
        let float_type = unsafe {
            LLVMFloatType()
        };

        FloatType::new(float_type)
    }

    /// Creates a new `FloatType` which represents sixty four bits (eight bytes) for the global context.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use inkwell::context::Context;
    /// use inkwell::types::FloatType;
    ///
    /// let f64_type = FloatType::f64_type();
    ///
    /// assert_eq!(f64_type.get_context(), Context::get_global());
    /// ```
    pub fn f64_type() -> Self {
        let float_type = unsafe {
            LLVMDoubleType()
        };

        FloatType::new(float_type)
    }

    /// Gets the `FloatType` representing a 80 bit width. It will be assigned the global context.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use inkwell::context::Context;
    /// use inkwell::types::FloatType;
    ///
    /// let x86_f80_type = FloatType::x86_f80_type();
    ///
    /// assert_eq!(x86_f80_type.get_context(), Context::get_global());
    /// ```
    pub fn x86_f80_type() -> FloatType {
        let f128_type = unsafe {
            LLVMX86FP80Type()
        };

        FloatType::new(f128_type)
    }

    /// Creates a new `FloatType` which represents one hundred and twenty eight bits (sixteen bytes) for the global context.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use inkwell::context::Context;
    /// use inkwell::types::FloatType;
    ///
    /// let f128_type = FloatType::f128_type();
    ///
    /// assert_eq!(f128_type.get_context(), Context::get_global());
    /// ```
    // IEEE 754-2008’s binary128 floats according to https://internals.rust-lang.org/t/pre-rfc-introduction-of-half-and-quadruple-precision-floats-f16-and-f128/7521
    pub fn f128_type() -> Self {
        let float_type = unsafe {
            LLVMFP128Type()
        };

        FloatType::new(float_type)
    }

    /// Creates a new `FloatType` which represents one hundred and twenty eight bits (sixteen bytes) for the current context. PPC is two 64 bits side by side rather than one single 128 bit float.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use inkwell::context::Context;
    /// use inkwell::types::FloatType;
    ///
    /// let f128_type = FloatType::ppc_f128_type();
    ///
    /// assert_eq!(f128_type.get_context(), Context::get_global());
    /// ```
    // Two 64 bits according to https://internals.rust-lang.org/t/pre-rfc-introduction-of-half-and-quadruple-precision-floats-f16-and-f128/7521
    pub fn ppc_f128_type() -> Self {
        let float_type = unsafe {
            LLVMPPCFP128Type()
        };

        FloatType::new(float_type)
    }

    pub fn print_to_string(&self) -> LLVMString {
        self.float_type.print_to_string()
    }

    // See Type::print_to_stderr note on 5.0+ status
    #[cfg(any(feature = "llvm3-7", feature = "llvm3-8", feature = "llvm3-9", feature = "llvm4-0"))]
    pub fn print_to_stderr(&self) {
        self.float_type.print_to_stderr()
    }

    pub fn get_undef(&self) -> FloatValue {
        FloatValue::new(self.float_type.get_undef())
    }

    pub fn create_generic_value(&self, value: f64) -> GenericValue {
        let value = unsafe {
            LLVMCreateGenericValueOfFloat(self.as_type_ref(), value)
        };

        GenericValue::new(value)
    }
}

impl AsTypeRef for FloatType {
    fn as_type_ref(&self) -> LLVMTypeRef {
        self.float_type.type_
    }
}
