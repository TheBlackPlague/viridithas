use std::fmt::Display;

use crate::{
    definitions::Square,
    lookups, magic,
    piece::{Colour, Piece, PieceType},
};

pub const BB_RANK_1: u64 = 0x0000_0000_0000_00FF;
pub const BB_RANK_2: u64 = 0x0000_0000_0000_FF00;
pub const BB_RANK_3: u64 = 0x0000_0000_00FF_0000;
pub const BB_RANK_4: u64 = 0x0000_0000_FF00_0000;
pub const BB_RANK_5: u64 = 0x0000_00FF_0000_0000;
pub const BB_RANK_6: u64 = 0x0000_FF00_0000_0000;
pub const BB_RANK_7: u64 = 0x00FF_0000_0000_0000;
pub const BB_RANK_8: u64 = 0xFF00_0000_0000_0000;
pub const BB_FILE_A: u64 = 0x0101_0101_0101_0101;
pub const BB_FILE_B: u64 = 0x0202_0202_0202_0202;
pub const BB_FILE_C: u64 = 0x0404_0404_0404_0404;
pub const BB_FILE_D: u64 = 0x0808_0808_0808_0808;
pub const BB_FILE_E: u64 = 0x1010_1010_1010_1010;
pub const BB_FILE_F: u64 = 0x2020_2020_2020_2020;
pub const BB_FILE_G: u64 = 0x4040_4040_4040_4040;
pub const BB_FILE_H: u64 = 0x8080_8080_8080_8080;
pub const BB_NONE: u64 = 0x0000_0000_0000_0000;
pub const BB_ALL: u64 = 0xFFFF_FFFF_FFFF_FFFF;
pub const BB_LIGHT_SQUARES: u64 = 0x55AA_55AA_55AA_55AA;
pub const BB_DARK_SQUARES: u64 = 0xAA55_AA55_AA55_AA55;

pub const LIGHT_SQUARE: bool = true;
pub const DARK_SQUARE: bool = false;

pub static BB_RANKS: [u64; 8] =
    [BB_RANK_1, BB_RANK_2, BB_RANK_3, BB_RANK_4, BB_RANK_5, BB_RANK_6, BB_RANK_7, BB_RANK_8];

pub static BB_FILES: [u64; 8] =
    [BB_FILE_A, BB_FILE_B, BB_FILE_C, BB_FILE_D, BB_FILE_E, BB_FILE_F, BB_FILE_G, BB_FILE_H];

/// least significant bit of a u64
/// ```
/// assert_eq!(3, bitboard::lsb(0b00001000));
/// ```
pub const fn lsb(x: u64) -> u64 {
    x.trailing_zeros() as u64
}

/// first set square of a u64
pub const fn first_square(x: u64) -> Square {
    #![allow(clippy::cast_possible_truncation)]
    Square::new(x.trailing_zeros() as u8)
}

/// Iterator over the squares of a bitboard.
/// The squares are returned in increasing order.
pub struct BitLoop {
    value: u64,
}

impl BitLoop {
    pub const fn new(value: u64) -> Self {
        Self { value }
    }
}

impl Iterator for BitLoop {
    type Item = Square;

    fn next(&mut self) -> Option<Self::Item> {
        if self.value == 0 {
            None
        } else {
            // faster if we have bmi (maybe)
            // SOUNDNESS: the trailing_zeros of a u64 cannot exceed 64, which is less than u8::MAX
            #[allow(clippy::cast_possible_truncation)]
            let lsb: u8 = self.value.trailing_zeros() as u8;
            self.value &= self.value - 1;
            Some(Square::new(lsb))
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BitBoard {
    pieces: [u64; 6],
    colours: [u64; 2],
}

impl BitBoard {
    pub const NULL: Self = Self::new(0, 0, 0, 0, 0, 0, 0, 0);

    #[allow(clippy::too_many_arguments, clippy::many_single_char_names)]
    pub const fn new(
        p: u64,
        n: u64,
        b: u64,
        r: u64,
        q: u64,
        k: u64,
        white: u64,
        black: u64,
    ) -> Self {
        Self { pieces: [p, n, b, r, q, k], colours: [white, black] }
    }

    pub const fn king<const IS_WHITE: bool>(&self) -> u64 {
        self.all_kings() & self.our_pieces::<IS_WHITE>()
    }

    pub const fn pawns<const IS_WHITE: bool>(&self) -> u64 {
        self.all_pawns() & self.our_pieces::<IS_WHITE>()
    }

    pub const fn occupied_co(&self, colour: Colour) -> u64 {
        self.colours[colour.index()]
    }

    pub const fn their_pieces<const IS_WHITE: bool>(&self) -> u64 {
        if IS_WHITE {
            self.colours[Colour::BLACK.index()]
        } else {
            self.colours[Colour::WHITE.index()]
        }
    }

    pub const fn our_pieces<const IS_WHITE: bool>(&self) -> u64 {
        if IS_WHITE {
            self.colours[Colour::WHITE.index()]
        } else {
            self.colours[Colour::BLACK.index()]
        }
    }

    pub const fn rookqueen<const IS_WHITE: bool>(&self) -> u64 {
        (self.all_rooks() | self.all_queens()) & self.our_pieces::<IS_WHITE>()
    }

    pub const fn bishopqueen<const IS_WHITE: bool>(&self) -> u64 {
        (self.all_bishops() | self.all_queens()) & self.our_pieces::<IS_WHITE>()
    }

    pub const fn minors<const IS_WHITE: bool>(&self) -> u64 {
        (self.all_bishops() | self.all_knights()) & self.our_pieces::<IS_WHITE>()
    }

    pub const fn majors<const IS_WHITE: bool>(&self) -> u64 {
        (self.all_rooks() | self.all_queens()) & self.our_pieces::<IS_WHITE>()
    }

    pub const fn empty(&self) -> u64 {
        !self.occupied()
    }

    pub const fn occupied(&self) -> u64 {
        self.colours[Colour::WHITE.index()] | self.colours[Colour::BLACK.index()]
    }

    pub const fn knights<const IS_WHITE: bool>(&self) -> u64 {
        self.all_knights() & self.our_pieces::<IS_WHITE>()
    }

    pub const fn rooks<const IS_WHITE: bool>(&self) -> u64 {
        self.all_rooks() & self.our_pieces::<IS_WHITE>()
    }

    pub const fn bishops<const IS_WHITE: bool>(&self) -> u64 {
        self.all_bishops() & self.our_pieces::<IS_WHITE>()
    }

    pub const fn queens<const IS_WHITE: bool>(&self) -> u64 {
        self.all_queens() & self.our_pieces::<IS_WHITE>()
    }

    pub const fn all_pawns(&self) -> u64 {
        self.pieces[PieceType::PAWN.index()]
    }

    pub const fn all_knights(&self) -> u64 {
        self.pieces[PieceType::KNIGHT.index()]
    }

    pub const fn all_bishops(&self) -> u64 {
        self.pieces[PieceType::BISHOP.index()]
    }

    pub const fn all_rooks(&self) -> u64 {
        self.pieces[PieceType::ROOK.index()]
    }

    pub const fn all_queens(&self) -> u64 {
        self.pieces[PieceType::QUEEN.index()]
    }

    pub const fn all_kings(&self) -> u64 {
        self.pieces[PieceType::KING.index()]
    }

    pub const fn bishops_sqco<const IS_WHITE: bool, const IS_LSB: bool>(&self) -> u64 {
        if IS_LSB {
            self.bishops::<IS_WHITE>() & BB_LIGHT_SQUARES
        } else {
            self.bishops::<IS_WHITE>() & BB_DARK_SQUARES
        }
    }

    pub fn reset(&mut self) {
        *self = Self::NULL;
    }

    pub fn move_piece(&mut self, from_to_bb: u64, piece: Piece) {
        self.pieces[piece.piece_type().index()] ^= from_to_bb;
        self.colours[piece.colour().index()] ^= from_to_bb;
    }

    pub fn set_piece_at(&mut self, sq: Square, piece: Piece) {
        let sq_bb = sq.bitboard();
        self.pieces[piece.piece_type().index()] |= sq_bb;
        self.colours[piece.colour().index()] |= sq_bb;
    }

    pub fn clear_piece_at(&mut self, sq: Square, piece: Piece) {
        let sq_bb = sq.bitboard();
        self.pieces[piece.piece_type().index()] &= !sq_bb;
        self.colours[piece.colour().index()] &= !sq_bb;
    }

    pub const fn any_pawns(&self) -> bool {
        self.all_pawns() != 0
    }

    pub const fn piece_bb(&self, piece: Piece) -> u64 {
        self.pieces[piece.piece_type().index()] & self.colours[piece.colour().index()]
    }

    pub const fn of_type(&self, piece_type: PieceType) -> u64 {
        self.pieces[piece_type.index()]
    }

    pub fn pawn_attacks<const IS_WHITE: bool>(&self) -> u64 {
        if IS_WHITE {
            self.pawns::<true>().north_east_one() | self.pawns::<true>().north_west_one()
        } else {
            self.pawns::<false>().south_east_one() | self.pawns::<false>().south_west_one()
        }
    }

    pub fn all_attackers_to_sq(&self, sq: Square, occupied: u64) -> u64 {
        let sq_bb = sq.bitboard();
        let black_pawn_attackers = pawn_attacks::<true>(sq_bb) & self.pawns::<false>();
        let white_pawn_attackers = pawn_attacks::<false>(sq_bb) & self.pawns::<true>();
        let knight_attackers =
            attacks::<{ PieceType::KNIGHT.inner() }>(sq, BB_NONE) & (self.all_knights());
        let diag_attackers = attacks::<{ PieceType::BISHOP.inner() }>(sq, occupied)
            & (self.all_bishops() | self.all_queens());
        let orth_attackers = attacks::<{ PieceType::ROOK.inner() }>(sq, occupied)
            & (self.all_rooks() | self.all_queens());
        let king_attackers =
            attacks::<{ PieceType::KING.inner() }>(sq, BB_NONE) & (self.all_kings());
        black_pawn_attackers
            | white_pawn_attackers
            | knight_attackers
            | diag_attackers
            | orth_attackers
            | king_attackers
    }

    fn piece_at(&self, sq: Square) -> Piece {
        let sq_bb = sq.bitboard();
        let colour = if self.our_pieces::<true>() & sq_bb != 0 {
            Colour::WHITE
        } else if self.our_pieces::<false>() & sq_bb != 0 {
            Colour::BLACK
        } else {
            return Piece::EMPTY;
        };
        for piece in PieceType::all() {
            if self.pieces[piece.index()] & sq_bb != 0 {
                return Piece::new(colour, piece);
            }
        }
        panic!("Bit set in colour bitboard for {colour:?} but not in piece bitboards");
    }

    fn any_bbs_overlapping(&self) -> bool {
        if self.colours[0] & self.colours[1] != 0 {
            return true;
        }
        for i in 0..self.pieces.len() {
            for j in i + 1..self.pieces.len() {
                if self.pieces[i] & self.pieces[j] != 0 {
                    return true;
                }
            }
        }
        false
    }
}

pub trait BitHackExt {
    fn north_east_one(self) -> Self;
    fn north_west_one(self) -> Self;
    fn south_east_one(self) -> Self;
    fn south_west_one(self) -> Self;
    fn east_one(self) -> Self;
    fn west_one(self) -> Self;
    fn north_one(self) -> Self;
    fn south_one(self) -> Self;
    fn first_square(self) -> Square;
}

impl BitHackExt for u64 {
    fn north_east_one(self) -> Self {
        (self << 9) & !BB_FILE_A
    }
    fn north_west_one(self) -> Self {
        (self << 7) & !BB_FILE_H
    }
    fn south_east_one(self) -> Self {
        (self >> 7) & !BB_FILE_A
    }
    fn south_west_one(self) -> Self {
        (self >> 9) & !BB_FILE_H
    }
    fn east_one(self) -> Self {
        (self >> 1) & !BB_FILE_A
    }
    fn west_one(self) -> Self {
        (self << 1) & !BB_FILE_H
    }
    fn north_one(self) -> Self {
        self << 8
    }
    fn south_one(self) -> Self {
        self >> 8
    }
    fn first_square(self) -> Square {
        #![allow(clippy::cast_possible_truncation)]
        debug_assert!(self != 0, "Tried to get first square of empty bitboard");
        Square::new(self.trailing_zeros() as u8)
    }
}

pub fn attacks<const PIECE_TYPE: u8>(sq: Square, blockers: u64) -> u64 {
    debug_assert_ne!(PieceType::new(PIECE_TYPE), PieceType::PAWN);
    match PieceType::new(PIECE_TYPE) {
        PieceType::BISHOP => magic::get_bishop_attacks(sq, blockers),
        PieceType::ROOK => magic::get_rook_attacks(sq, blockers),
        PieceType::QUEEN => {
            magic::get_bishop_attacks(sq, blockers) | magic::get_rook_attacks(sq, blockers)
        }
        PieceType::KNIGHT => lookups::get_jumping_piece_attack::<{ PieceType::KNIGHT.inner() }>(sq),
        PieceType::KING => lookups::get_jumping_piece_attack::<{ PieceType::KING.inner() }>(sq),
        _ => panic!("Invalid piece type"),
    }
}

pub fn attacks_by_type(pt: PieceType, sq: Square, blockers: u64) -> u64 {
    match pt {
        PieceType::BISHOP => magic::get_bishop_attacks(sq, blockers),
        PieceType::ROOK => magic::get_rook_attacks(sq, blockers),
        PieceType::QUEEN => {
            magic::get_bishop_attacks(sq, blockers) | magic::get_rook_attacks(sq, blockers)
        }
        PieceType::KNIGHT => lookups::get_jumping_piece_attack::<{ PieceType::KNIGHT.inner() }>(sq),
        PieceType::KING => lookups::get_jumping_piece_attack::<{ PieceType::KING.inner() }>(sq),
        _ => panic!("Invalid piece type: {pt:?}"),
    }
}

pub fn pawn_attacks<const IS_WHITE: bool>(bb: u64) -> u64 {
    if IS_WHITE {
        bb.north_east_one() | bb.north_west_one()
    } else {
        bb.south_east_one() | bb.south_west_one()
    }
}

impl Display for BitBoard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for rank in (0..=7).rev() {
            for file in 0..=7 {
                let sq = Square::from_rank_file(rank, file);
                let piece = self.piece_at(sq);
                let piece_char = piece.char();
                if let Some(symbol) = piece_char {
                    write!(f, " {symbol}")?;
                } else {
                    write!(f, " .")?;
                }
            }
            writeln!(f)?;
        }
        if self.any_bbs_overlapping() {
            writeln!(f, "WARNING: Some bitboards are overlapping")?;
        }
        Ok(())
    }
}
