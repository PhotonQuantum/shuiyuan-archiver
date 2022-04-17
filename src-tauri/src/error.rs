use std::error::Error;
use std::fmt::{Debug, Display};

pub struct WrappedError {
    msg: String,
    inner: Box<dyn Error + Send + Sync>,
}

impl Display for WrappedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl Debug for WrappedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl Error for WrappedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.inner)
    }
}

#[derive(Debug)]
pub struct StringError(String);

impl Display for StringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for StringError {}

pub trait ErrorExt {
    type OkType;
    type ErrType: Error + Send + Sync;
    fn wrap_err(self, msg: impl Display) -> Result<Self::OkType, Self::ErrType>;
}

impl<T, E> ErrorExt for Result<T, E>
where
    E: Error + Send + Sync + 'static,
{
    type OkType = T;
    type ErrType = WrappedError;
    fn wrap_err(self, msg: impl Display) -> Result<Self::OkType, Self::ErrType> {
        self.map_err(|e| WrappedError {
            msg: msg.to_string(),
            inner: Box::new(e),
        })
    }
}

impl<T> ErrorExt for Option<T> {
    type OkType = T;
    type ErrType = StringError;

    fn wrap_err(self, msg: impl Display) -> Result<Self::OkType, Self::ErrType> {
        self.ok_or_else(|| StringError(msg.to_string()))
    }
}
