use itertools::Itertools;

use crate::chain::Float;

pub(super) const FLOAT_TWO_POW_64: Float = Float::from_bits(4_895_412_794_951_729_152_u64);
#[allow(unused)] // Actually used, but only in tests ATM
pub(super) const FLOAT_TWO_POW_128: Float = Float::from_bits(5_183_643_171_103_440_896_u64);

pub(crate) fn ufp_to_float<const TOT_SIZE: usize, const FRACT_SIZE: usize>(
    underlying: [u64; TOT_SIZE],
) -> Float {
    let mut words_reversed = underlying.iter().rev();
    if let Some((highest_nonzero_word_reverse_index, &highest_nonzero_word)) =
        words_reversed.find_position(|&word| *word != 0)
    {
        // TODO: consider optimizing using f64::from_bits (similarly to e.g. https://blog.m-ou.se/floats/)
        #[allow(clippy::cast_precision_loss)]
        let mut res = Float::from(highest_nonzero_word);
        if let Some(&next_to_highest_nonzero_word) = words_reversed.next() {
            #[allow(clippy::cast_precision_loss)]
            let next_to_highest_nonzero_word_float = Float::from(next_to_highest_nonzero_word);
            res += next_to_highest_nonzero_word_float / FLOAT_TWO_POW_64;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let highest_nonzero_word_scale = FLOAT_TWO_POW_64
            .powi((TOT_SIZE - FRACT_SIZE - 1) as i32 - highest_nonzero_word_reverse_index as i32);
        res *= highest_nonzero_word_scale;
        res
    } else {
        Float::zero()
    }
}
