// use crate::{board::Board, piece::PieceType};

use super::network::Align;

/// Activations of the hidden layer.
#[derive(Debug, Clone, Copy)]
pub struct Accumulator<const HIDDEN: usize> {
    pub white: Align<[i16; HIDDEN]>,
    pub black: Align<[i16; HIDDEN]>,
}

impl<const HIDDEN: usize> Accumulator<HIDDEN> {
    /// Initializes the accumulator with the given bias.
    pub fn init(&mut self, bias: &[i16; HIDDEN]) {
        self.white.copy_from_slice(bias);
        self.black.copy_from_slice(bias);
    }
}
