use crate::fp::{
    I192X192, I256X256, I256X320, U128X128, U192X192, U192X64, U256X256, U256X320, U320X128,
    U320X64,
};
use rug::ops::CompleteRound;
use std::fmt;
use std::fmt::Formatter;

impl fmt::Display for U128X128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            rug::Float::with_val(256, self.upper_part())
                + (rug::Float::with_val(256, self.lower_part()) >> 128)
        )
    }
}

impl fmt::Debug for U128X128 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for U192X192 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let two_pow_64 = rug::Float::with_val(384, 1u128 << 64);
        let two_pow_128 = (&two_pow_64 * &two_pow_64).complete(384);
        let two_pow_192 = (&two_pow_128 * &two_pow_64).complete(384);
        let value_rug = rug::Float::with_val(384, self.0 .0[5]) * &two_pow_128
            + rug::Float::with_val(384, self.0 .0[4]) * &two_pow_64
            + rug::Float::with_val(384, self.0 .0[3])
            + rug::Float::with_val(384, self.0 .0[2]) / &two_pow_64
            + rug::Float::with_val(384, self.0 .0[1]) / &two_pow_128
            + rug::Float::with_val(384, self.0 .0[0]) / &two_pow_192;
        write!(f, "{value_rug}")
    }
}

impl fmt::Display for I192X192 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let two_pow_64 = rug::Float::with_val(384, 1u128 << 64);
        let two_pow_128 = (&two_pow_64 * &two_pow_64).complete(384);
        let two_pow_192 = (&two_pow_128 * &two_pow_64).complete(384);
        let mut value_rug = rug::Float::with_val(384, self.value.0 .0[5]) * &two_pow_128
            + rug::Float::with_val(384, self.value.0 .0[4]) * &two_pow_64
            + rug::Float::with_val(384, self.value.0 .0[3])
            + rug::Float::with_val(384, self.value.0 .0[2]) / &two_pow_64
            + rug::Float::with_val(384, self.value.0 .0[1]) / &two_pow_128
            + rug::Float::with_val(384, self.value.0 .0[0]) / &two_pow_192;
        if !self.non_negative {
            value_rug = -value_rug;
        }
        write!(f, "{value_rug}")
    }
}

impl fmt::Display for U256X256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let two_pow_64 = rug::Float::with_val(512, 1u128 << 64);
        let two_pow_128 = (&two_pow_64 * &two_pow_64).complete(512);
        let two_pow_192 = (&two_pow_128 * &two_pow_64).complete(512);
        let two_pow_256 = (&two_pow_192 * &two_pow_64).complete(512);
        let value_rug = rug::Float::with_val(512, self.0 .0[7]) * &two_pow_192
            + rug::Float::with_val(512, self.0 .0[6]) * &two_pow_128
            + rug::Float::with_val(512, self.0 .0[5]) * &two_pow_64
            + rug::Float::with_val(512, self.0 .0[4])
            + rug::Float::with_val(512, self.0 .0[3]) / &two_pow_64
            + rug::Float::with_val(512, self.0 .0[2]) / &two_pow_128
            + rug::Float::with_val(512, self.0 .0[1]) / &two_pow_192
            + rug::Float::with_val(512, self.0 .0[0]) / &two_pow_256;
        write!(f, "{value_rug}")
    }
}

impl fmt::Debug for U256X256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for I256X256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let two_pow_64 = rug::Float::with_val(512, 1u128 << 64);
        let two_pow_128 = (&two_pow_64 * &two_pow_64).complete(512);
        let two_pow_192 = (&two_pow_128 * &two_pow_64).complete(512);
        let two_pow_256 = (&two_pow_192 * &two_pow_64).complete(512);
        let mut value_rug = rug::Float::with_val(512, self.value.0 .0[7]) * &two_pow_192
            + rug::Float::with_val(512, self.value.0 .0[6]) * &two_pow_128
            + rug::Float::with_val(512, self.value.0 .0[5]) * &two_pow_64
            + rug::Float::with_val(512, self.value.0 .0[4])
            + rug::Float::with_val(512, self.value.0 .0[3]) / &two_pow_64
            + rug::Float::with_val(512, self.value.0 .0[2]) / &two_pow_128
            + rug::Float::with_val(512, self.value.0 .0[1]) / &two_pow_192
            + rug::Float::with_val(512, self.value.0 .0[0]) / &two_pow_256;

        if !self.non_negative {
            value_rug = -value_rug;
        }

        write!(f, "{value_rug}")
    }
}

impl fmt::Debug for U192X192 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for U192X64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let two_pow_64 = rug::Float::with_val(256, 1u128 << 64);
        let two_pow_128 = (&two_pow_64 * &two_pow_64).complete(256);
        let value_rug = rug::Float::with_val(256, self.0 .0[3]) * &two_pow_128
            + rug::Float::with_val(256, self.0 .0[2]) * &two_pow_64
            + rug::Float::with_val(256, self.0 .0[1])
            + rug::Float::with_val(256, self.0 .0[0]) / &two_pow_64;
        write!(f, "{value_rug}")
    }
}

impl fmt::Debug for U192X64 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for U320X64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let two_pow_64 = rug::Float::with_val(384, 1u128 << 64);
        let two_pow_128 = (&two_pow_64 * &two_pow_64).complete(384);
        let two_pow_192 = (&two_pow_128 * &two_pow_64).complete(384);
        let two_pow_256 = (&two_pow_192 * &two_pow_64).complete(384);
        let value_rug = rug::Float::with_val(384, self.0 .0[5]) * &two_pow_256
            + rug::Float::with_val(384, self.0 .0[4]) * &two_pow_192
            + rug::Float::with_val(384, self.0 .0[3]) * &two_pow_128
            + rug::Float::with_val(384, self.0 .0[2]) * &two_pow_64
            + rug::Float::with_val(384, self.0 .0[1])
            + rug::Float::with_val(384, self.0 .0[0]) / &two_pow_64;
        write!(f, "{value_rug}")
    }
}

impl fmt::Display for U320X128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let two_pow_64 = rug::Float::with_val(448, 1u128 << 64);
        let two_pow_128 = (&two_pow_64 * &two_pow_64).complete(448);
        let two_pow_192 = (&two_pow_128 * &two_pow_64).complete(448);
        let two_pow_256 = (&two_pow_192 * &two_pow_64).complete(448);
        let value_rug = rug::Float::with_val(448, self.0 .0[6]) * &two_pow_256
            + rug::Float::with_val(448, self.0 .0[5]) * &two_pow_192
            + rug::Float::with_val(448, self.0 .0[4]) * &two_pow_128
            + rug::Float::with_val(448, self.0 .0[3]) * &two_pow_64
            + rug::Float::with_val(448, self.0 .0[2])
            + rug::Float::with_val(448, self.0 .0[1]) / &two_pow_64
            + rug::Float::with_val(448, self.0 .0[0]) / &two_pow_128;
        write!(f, "{value_rug}")
    }
}
impl fmt::Debug for U320X128 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for U256X320 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let two_pow_64 = rug::Float::with_val(576, 1u128 << 64);
        let two_pow_128 = (&two_pow_64 * &two_pow_64).complete(576);
        let two_pow_192 = (&two_pow_128 * &two_pow_64).complete(576);
        let two_pow_256 = (&two_pow_192 * &two_pow_64).complete(576);
        let two_pow_320 = (&two_pow_256 * &two_pow_64).complete(576);
        let value_rug = rug::Float::with_val(576, self.0 .0[8]) * &two_pow_192
            + rug::Float::with_val(576, self.0 .0[7]) * &two_pow_128
            + rug::Float::with_val(576, self.0 .0[6]) * &two_pow_64
            + rug::Float::with_val(576, self.0 .0[5])
            + rug::Float::with_val(576, self.0 .0[4]) / &two_pow_64
            + rug::Float::with_val(576, self.0 .0[3]) / &two_pow_128
            + rug::Float::with_val(576, self.0 .0[2]) / &two_pow_192
            + rug::Float::with_val(576, self.0 .0[1]) / &two_pow_256
            + rug::Float::with_val(576, self.0 .0[0]) / &two_pow_320;
        write!(f, "{value_rug}")
    }
}

impl fmt::Display for I256X320 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let two_pow_64 = rug::Float::with_val(576, 1u128 << 64);
        let two_pow_128 = (&two_pow_64 * &two_pow_64).complete(576);
        let two_pow_192 = (&two_pow_128 * &two_pow_64).complete(576);
        let two_pow_256 = (&two_pow_192 * &two_pow_64).complete(576);
        let two_pow_320 = (&two_pow_256 * &two_pow_64).complete(576);
        let mut value_rug = rug::Float::with_val(576, self.value.0 .0[8]) * &two_pow_192
            + rug::Float::with_val(576, self.value.0 .0[7]) * &two_pow_128
            + rug::Float::with_val(576, self.value.0 .0[6]) * &two_pow_64
            + rug::Float::with_val(576, self.value.0 .0[5])
            + rug::Float::with_val(576, self.value.0 .0[4]) / &two_pow_64
            + rug::Float::with_val(576, self.value.0 .0[3]) / &two_pow_128
            + rug::Float::with_val(576, self.value.0 .0[2]) / &two_pow_192
            + rug::Float::with_val(576, self.value.0 .0[1]) / &two_pow_256
            + rug::Float::with_val(576, self.value.0 .0[0]) / &two_pow_320;

        if !self.non_negative {
            value_rug = -value_rug;
        }

        write!(f, "{value_rug}")
    }
}
