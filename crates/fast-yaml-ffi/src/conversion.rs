use crate::error::FfiResult;

/// Trait for converting Rust types to foreign types.
///
/// This trait provides a generic interface for converting Rust values
/// into representations suitable for FFI (Foreign Function Interface).
///
/// # Type Parameters
///
/// - `T`: The target foreign type
///
/// # Examples
///
/// ```ignore
/// impl ToFfi<PyObject> for Value {
///     fn to_ffi(&self, context: &Context) -> FfiResult<PyObject> {
///         // Conversion logic
///     }
/// }
/// ```
pub trait ToFfi<T> {
    /// Convert this value to the foreign type.
    ///
    /// # Errors
    ///
    /// Returns `FfiError` if the conversion fails.
    fn to_ffi(&self) -> FfiResult<T>;
}

/// Trait for converting foreign types to Rust types.
///
/// This trait provides a generic interface for converting foreign values
/// into Rust representations.
///
/// # Type Parameters
///
/// - `T`: The source foreign type
///
/// # Examples
///
/// ```ignore
/// impl FromFfi<PyObject> for Value {
///     fn from_ffi(obj: &PyObject, context: &Context) -> FfiResult<Self> {
///         // Conversion logic
///     }
/// }
/// ```
pub trait FromFfi<T>: Sized {
    /// Convert from the foreign type to this Rust type.
    ///
    /// # Errors
    ///
    /// Returns `FfiError` if the conversion fails.
    fn from_ffi(foreign: &T) -> FfiResult<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct TestType(i32);

    impl ToFfi<i32> for TestType {
        fn to_ffi(&self) -> FfiResult<i32> {
            Ok(self.0)
        }
    }

    impl FromFfi<i32> for TestType {
        fn from_ffi(foreign: &i32) -> FfiResult<Self> {
            Ok(TestType(*foreign))
        }
    }

    #[test]
    fn test_to_ffi() {
        let val = TestType(42);
        let result = val.to_ffi().unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_from_ffi() {
        let foreign = 42;
        let result = TestType::from_ffi(&foreign).unwrap();
        assert_eq!(result, TestType(42));
    }
}
