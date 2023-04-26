use std::convert::Infallible;

use crate::chain;
use strum::EnumCount as _;
use thiserror::Error;

pub fn describe_error_code(value: i32) -> String {
    ErrorCode::try_from(value)
        .map_or_else(|_| "<invalid error code>".to_string(), |e| e.to_string())
}

include!(concat!(env!("OUT_DIR"), "/source_files_list.rs"));
// Ensure that error discriminant will fit into error code bits
static_assertions::const_assert!(ErrorKindDiscriminants::COUNT <= (CODE_MASK as usize));
// Ensure source file index would fit into file bits.
// Please note that index 0 is reserved for "unknown file", just in case,
// so `SOURCE_FILES_COUNT` should be strictly less than `FILE_MASK`, which is maximum
// possible file index
static_assertions::const_assert!(SOURCE_FILES_COUNT < (FILE_MASK as usize));
/// Creates error object with location info filled from macro invocation location
///
/// # Arguments
/// * `$kind` - expression which should produce `ErrorKind` value
#[macro_export]
macro_rules! error_here {
    ($kind:expr) => {{
        // Statically ensure we have enough space to store line number
        static_assertions::const_assert!(line!() <= $crate::dex::MAX_LINE);
        // Use `Location::caller()` instead of `file!()`+`line!()`+`column!()` macros
        // 'cause `caller` is subject to `#[track_caller]` attribute,
        // while macros are not, or at least not guaranteed
        let loc = std::panic::Location::caller();
        $crate::dex::Error {
            kind: ($kind).into(),
            file: loc.file(),
            line: loc.line(),
            column: loc.column(),
        }
    }};
}
/// Construct blockchain-specific custom error with this enum.
/// Adds few necessary derives, incl. generating twin enum
/// with pure discriminants
#[macro_export]
macro_rules! custom_error {
    (
        $(#[$attr:meta])*
        pub enum Error {
            $($enum_body:tt)*
        }
    ) => {
        $(#[$attr])*
        #[derive(thiserror::Error, Debug, strum_macros::EnumDiscriminants)]
        #[strum_discriminants(
            vis(pub(crate)),
            derive(
                strum_macros::IntoStaticStr,
                strum_macros::EnumCount,
                strum_macros::FromRepr
            )
        )]
        pub enum Error {
            $($enum_body)*
        }
    };
}

#[macro_export]
macro_rules! ensure_here {
    ($cond:expr, $err:expr) => {
        $crate::ensure!($cond, $crate::error_here!($err))
    };
}

// Public only to allow use in `error_here!` macro
#[doc(hidden)]
pub const MAX_LINE: u32 = LINE_MASK as u32;

const LINE_BITS: u32 = 12;
const LINE_MASK: i32 = (1i32 << LINE_BITS) - 1;
const LINE_OFFSET: u32 = 0;

const FILE_BITS: u32 = 8;
const FILE_MASK: i32 = (1i32 << FILE_BITS) - 1;
const FILE_OFFSET: u32 = LINE_BITS + LINE_OFFSET;

const CODE_BITS: u32 = 8;
const CODE_MASK: i32 = (1i32 << CODE_BITS) - 1;
const CODE_OFFSET: u32 = FILE_BITS + FILE_OFFSET;

const TOTAL_BITS: u32 = CODE_BITS + CODE_OFFSET;
/// Error object which contains both error kind and its spawn location
#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub file: &'static str,
    pub line: u32,
    pub column: u32,
}

impl Error {
    /// Convert error into tightly-packed error code representation of a single 32-bit unsigned integer
    pub fn error_code(&self) -> ErrorCode {
        ErrorDesc::from(self).into()
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Error {}: {}", self.error_code().0, self.kind))
    }
}

impl std::error::Error for Error {}
/// Error which is used only to describe cases where input integer cannot be converted to `ErrorCode`
#[derive(Error, Debug)]
#[error("Input value is out of allowed range")]
pub struct ErrorCodeOutOfRangeError;

#[cfg_attr(test, derive(Copy, Clone, Debug, PartialEq, Eq))]
enum ErrorGroup {
    Unknown,
    Standard(ErrorKindDiscriminants),
    Custom(chain::ErrorDiscriminants),
}

#[cfg_attr(test, derive(Copy, Clone, Debug, PartialEq, Eq))]
struct ErrorDesc {
    error: ErrorGroup,
    file: &'static str,
    line: u32,
}

impl From<&Error> for ErrorDesc {
    fn from(error: &Error) -> Self {
        Self {
            error: match error.kind {
                ErrorKind::Custom(ref kind) => ErrorGroup::Custom(kind.into()),
                ref other => ErrorGroup::Standard(other.into()),
            },
            file: error.file,
            line: error.line,
        }
    }
}

impl std::fmt::Display for ErrorDesc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.error {
            ErrorGroup::Unknown => "<unknown error>",
            ErrorGroup::Standard(ref kind) => kind.into(),
            ErrorGroup::Custom(ref kind) => kind.into(),
        };
        let file = self.file;
        let line = self.line;
        f.write_fmt(format_args!("Error {kind} at \"{file}\":{line}"))
    }
}

impl From<ErrorDesc> for i32 {
    fn from(desc: ErrorDesc) -> Self {
        // Convert into discriminant bits
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let code = match desc.error {
            // Note: Custom kind is turned into Custom group, so if we meet ErrorKind::Custom,
            // it's an error
            ErrorGroup::Unknown | ErrorGroup::Standard(ErrorKindDiscriminants::Custom) => CODE_MASK,
            ErrorGroup::Standard(kind) => kind as i32,
            ErrorGroup::Custom(kind) => (kind as i32) + (ErrorKindDiscriminants::COUNT as i32),
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let file = (SOURCE_FILES
            .iter()
            .position(|p| *p == desc.file)
            .map_or(0, |i| i + 1) as i32)
            & FILE_MASK;
        #[allow(clippy::cast_possible_wrap)]
        let line = (desc.line as i32) & LINE_MASK;

        ErrorCode::MIN | (code << CODE_OFFSET) | (file << FILE_OFFSET) | (line << LINE_OFFSET)
    }
}

impl TryFrom<i32> for ErrorDesc {
    type Error = ErrorCodeOutOfRangeError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        if !(ErrorCode::MIN..=ErrorCode::MAX).contains(&value) {
            return Err(ErrorCodeOutOfRangeError);
        }

        let code = (value >> CODE_OFFSET) & CODE_MASK;
        let file = (value >> FILE_OFFSET) & FILE_MASK;
        let line = (value >> LINE_OFFSET) & LINE_MASK;

        #[allow(clippy::cast_sign_loss)]
        let file = SOURCE_FILES
            .get((file - 1) as usize)
            .map_or("<unknown file>", |e| *e);

        #[allow(clippy::cast_sign_loss)]
        let error = if let Some(kind) = ErrorKindDiscriminants::from_repr(code as usize) {
            match kind {
                ErrorKindDiscriminants::Custom => ErrorGroup::Unknown,
                other => ErrorGroup::Standard(other),
            }
        } else if let Some(kind) =
            chain::ErrorDiscriminants::from_repr((code as usize) - ErrorKindDiscriminants::COUNT)
        {
            ErrorGroup::Custom(kind)
        } else {
            ErrorGroup::Unknown
        };

        #[allow(clippy::cast_sign_loss)]
        Ok(Self {
            error,
            file,
            line: line as u32,
        })
    }
}

/// Simple wrapper around u32 error code which allows it to be `Display`'ed and `Debug`'ed
#[derive(Copy, Clone, Debug)]
pub struct ErrorCode(i32);

impl ErrorCode {
    pub fn integer(&self) -> i32 {
        self.0
    }
    /// Smallest possible value for inner error code integer
    pub const MIN: i32 = !((1i32 << TOTAL_BITS) - 1);
    /// Largest possible value for inner error code integer
    pub const MAX: i32 = -1i32; // all '1's
}

impl TryFrom<i32> for ErrorCode {
    type Error = ErrorCodeOutOfRangeError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(Self(ErrorDesc::try_from(value)?.into()))
    }
}

impl From<ErrorDesc> for ErrorCode {
    fn from(desc: ErrorDesc) -> Self {
        Self(desc.into())
    }
}

impl From<&ErrorCode> for ErrorDesc {
    fn from(code: &ErrorCode) -> Self {
        ErrorDesc::try_from(code.0).expect("Conversion from ErrorCode should never fail!")
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ErrorDesc::from(self).fmt(f)
    }
}

#[derive(Error, strum_macros::EnumDiscriminants)]
#[strum_discriminants(
    vis(pub(self)),
    derive(
        strum_macros::IntoStaticStr,
        strum_macros::EnumCount,
        strum_macros::FromRepr
    )
)]
pub enum ErrorKind {
    /// Blockchain-specific error
    /// Implemented as box ATM because parametrization of Error causes significant code clutter
    /// which cannot be remedied without associated type defaults
    #[error("{0}")]
    Custom(chain::Error),
    // Storage errors.
    #[error("Account not registered")]
    AccountNotRegistered,
    /// Should actually be Dex error, just not used there due to account removal logic being blockchain-specific
    #[error("Account's tokens storage not empty")]
    TokensStorageNotEmpty,
    // Accounts.
    #[error("Token not registered")]
    TokenNotRegistered,
    #[error("Not enough tokens in deposit")]
    NotEnoughTokens,
    #[error("Non-zero token balance")]
    NonZeroTokenBalance,
    #[error("Illegal withdraw amount")]
    IllegalWithdrawAmount,
    // Action errors
    #[error("Deposit sender must be transaction signer/initiator in order to perform batch actions as part of deposit")]
    DepositSenderMustBeSigner,
    #[error("`RegisterAccount` action can be only first in batch")]
    UnexpectedRegisterAccount,
    #[error("`Deposit` action already handled, should be present in batch exactly once")]
    DepositAlreadyHandled,
    #[error("`Deposit` action not handled, should be present in batch exactly once")]
    DepositNotHandled,
    #[error("`Deposit` action not allowed in this batch action context")]
    DepositNotAllowed,
    #[error("Operation cannot be performed at this moment - token withdraw is in progress. Please retry later")]
    WithdrawInProgress,
    #[error("Depositing such amounts would cause overflow. Total supply of the token exceeds max allowed FT total supply.")]
    DepositWouldOverflow,
    // Action result.
    #[error("Wrong action result type")]
    WrongActionResult,
    // Swap
    #[error("Slippage error")]
    Slippage,
    #[error("At least one swap")]
    AtLeastOneSwap,
    #[error("Insufficient liquidity in the pool to perform the swap")]
    InsufficientLiquidity,
    #[error("Swap amount too small")]
    SwapAmountTooSmall,
    #[error("Swap amount too large")]
    SwapAmountTooLarge,
    #[error("Invalid params")]
    InvalidParams,
    // pool manage
    #[error("Liquidity pool not registered")]
    PoolNotRegistered,
    #[error("Token duplicated")]
    TokenDuplicates,
    // owner or allowed address
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Guard change state denied")]
    GuardChangeStateDenied,
    #[error("Illegal fee")]
    IllegalFee,
    // Input
    #[error("Wrong token ratio")]
    WrongRatio,
    #[error("Resulting liquidity is too small")]
    LiquidityTooSmall,
    #[error("Resulting liquidity is too big")]
    LiquidityTooBig,
    #[error("Position already exists")]
    PositionAlreadyExists,
    #[error("Position does not exist")]
    PositionDoesNotExist,
    #[error("User has opened positions")]
    UserHasPositions,
    #[error("Not your position")]
    NotYourPosition,
    // Math errors
    #[error("Numeric conversion error: overflow - source number cannot fit into destination")]
    ConvOverflow,
    #[error("Numeric conversion error: source number is NaN")]
    ConvSourceNaN,
    #[error("Numeric conversion error: attempt to convert negative number to unsigned")]
    ConvNegativeToUnsigned,
    #[error(
        "Numeric conversion error: loss of precision, lower digits of source number truncated"
    )]
    ConvPrecisionLoss,
    // Payable API managment
    #[error("Payable API suspended")]
    PayableAPISuspended,
    // Internal logic errors
    #[error("Tick not found")]
    InternalTickNotFound,
    #[error("Tick not deleted")]
    InternalTickNotDeleted,
    #[error("Evaluated deposited amount is larger than specified max limit.")]
    InternalDepositMoreThanMax,
    #[error("Logic error: number of picked pools doesnt' match number of top pools")]
    InternalTopPoolsNumberMismatch,
    #[error("Internal logic error")]
    InternalLogicError,
    #[error("Tick value is either too large or too small")]
    PriceTickOutOfBounds,
}

// Custom debug implementation to not use `derive`, because it blows up binary size
impl std::fmt::Debug for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

/// We need this stub conversion - sometimes we get
/// infallible conversions where we don't expect them
impl From<Infallible> for ErrorKind {
    fn from(i: Infallible) -> Self {
        match i {}
    }
}

impl From<chain::Error> for ErrorKind {
    fn from(error: chain::Error) -> Self {
        ErrorKind::Custom(error)
    }
}

impl From<crate::fp::Error> for ErrorKind {
    fn from(err: crate::fp::Error) -> Self {
        match err {
            crate::fp::Error::NaN => Self::ConvSourceNaN,
            crate::fp::Error::NegativeToUnsigned => Self::ConvNegativeToUnsigned,
            crate::fp::Error::Overflow => Self::ConvOverflow,
            crate::fp::Error::PrecisionLoss => Self::ConvPrecisionLoss,
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
