use super::{Cause, Code, Error, safe_upgrade_url};

macro_rules! constructors {
    ($($(#[$doc:meta])* $name:ident => $code:ident;)*) => {
        impl Error {
            $(
                $(#[$doc])*
                pub fn $name(message: impl Into<String>) -> Self {
                    Self::new(Code::$code, message)
                }
            )*
        }
    };
}

constructors! {
    /// A malformed or unacceptable request. HTTP 400.
    invalid => InvalidArgument;
    /// A missing or unusable credential. HTTP 401.
    unauthorized => Unauthenticated;
    /// A feature or quota the caller's plan does not include. HTTP 402.
    payment_required => PaymentRequired;
    /// A valid credential that lacks the right. HTTP 403.
    forbidden => PermissionDenied;
    /// The addressed resource does not exist. HTTP 404.
    not_found => NotFound;
    /// The resource already exists or has moved on. HTTP 409.
    conflict => AlreadyExists;
    /// The system state rules the operation out. HTTP 412.
    failed => FailedPrecondition;
    /// A payload the server refuses to accept. HTTP 413.
    too_large => ResourceExhausted;
    /// The caller has spent its quota. HTTP 429.
    rate_limited => RateLimited;
    /// A verb the route does not serve. HTTP 405.
    not_allowed => MethodNotAllowed;
    /// A dependency that is down or still starting. HTTP 503.
    unavailable => Unavailable;
}

impl Error {
    /// Builds an error from a code and a message, with no cause.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), upgrade_url: None, cause: None }
    }

    /// A server-side fault. The cause is logged, never sent. HTTP 500.
    pub fn internal(message: impl Into<String>, cause: impl Into<Cause>) -> Self {
        Self { cause: Some(cause.into()), ..Self::new(Code::Internal, message) }
    }

    /// Sets where the caller can upgrade. An unsafe URL, such as `javascript:` or a
    /// protocol-relative one, leaves the field empty.
    pub fn with_upgrade_url(mut self, raw: impl Into<String>) -> Self {
        let raw = raw.into();
        self.upgrade_url = safe_upgrade_url(&raw).then_some(raw);
        self
    }
}
