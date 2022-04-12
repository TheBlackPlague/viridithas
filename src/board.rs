#![allow(dead_code)]

use std::{
    fmt::{Debug, Display, Formatter},
    mem,
};

use crate::{
    definitions::{Colour, Square120},
    lookups::{CASTLE_KEYS, PIECE_KEYS, SIDE_KEY, PIECE_COL, PIECE_BIG, PIECE_MIN, PIECE_MAJ, PIECE_VAL},
};
use crate::{
    definitions::{
        Castling, File, Piece, Rank, Undo, BOARD_N_SQUARES, MAX_GAME_MOVES,
    },
    lookups::{filerank_to_square, SQ64_TO_SQ120},
};

#[derive(Clone, Eq, PartialEq)]
pub struct Board {
    pieces: [u8; BOARD_N_SQUARES],
    pawns: [u64; 3],
    king_sq: [u8; 2],
    side: u8,
    ep_sq: u8,
    fifty_move_counter: u8,
    ply: usize,
    hist_ply: usize,
    key: u64,
    piece_counts: [u8; 13],
    big_piece_counts: [u8; 2],
    major_piece_counts: [u8; 2],
    minor_piece_counts: [u8; 2],
    material: [u16; 2],
    castle_perm: u8,
    history: [Undo; MAX_GAME_MOVES],
    p_list: [[u8; 10]; 13], // p_list[piece][N]
}

impl Board {
    pub fn new() -> Self {
        let mut out = Self {
            pieces: [Square120::NoSquare as u8; BOARD_N_SQUARES],
            pawns: [0; 3],
            king_sq: [Square120::NoSquare as u8; 2],
            side: 0,
            ep_sq: Square120::NoSquare as u8,
            fifty_move_counter: 0,
            ply: 0,
            hist_ply: 0,
            key: 0,
            piece_counts: [0; 13],
            big_piece_counts: [0; 2],
            major_piece_counts: [0; 2],
            minor_piece_counts: [0; 2],
            material: [0; 2],
            castle_perm: 0,
            history: [Undo::new(); MAX_GAME_MOVES],
            p_list: [[0; 10]; 13],
        };
        out.reset();
        out
    }

    pub fn generate_pos_key(&self) -> u64 {
        let mut key = 0;
        for sq in 0..BOARD_N_SQUARES {
            let piece = self.pieces[sq];
            if piece != Piece::Empty as u8 
            && piece != Square120::OffBoard as u8 
            && piece != Square120::NoSquare as u8 {
                debug_assert!(piece >= Piece::WP as u8 && piece <= Piece::BK as u8);
                key ^= PIECE_KEYS[piece as usize][sq];
            }
        }

        if self.side == Colour::White as u8 {
            key ^= SIDE_KEY;
        }

        if self.ep_sq != 0 {
            debug_assert!((self.ep_sq as usize) < BOARD_N_SQUARES);
            key ^= PIECE_KEYS[Piece::Empty as usize][self.ep_sq as usize];
        }

        debug_assert!(self.castle_perm <= 15);

        key ^= CASTLE_KEYS[self.castle_perm as usize];

        key
    }

    pub fn reset(&mut self) {
        self.pieces.fill(Square120::OffBoard as u8);
        for &i in &SQ64_TO_SQ120 {
            self.pieces[i as usize] = Piece::Empty as u8;
        }
        self.big_piece_counts.fill(0);
        self.major_piece_counts.fill(0);
        self.minor_piece_counts.fill(0);
        self.pawns.fill(0);
        self.piece_counts.fill(0);
        self.king_sq.fill(Square120::NoSquare as u8);
        self.side = Colour::Both as u8;
        self.ep_sq = Square120::NoSquare as u8;
        self.fifty_move_counter = 0;
        self.ply = 0;
        self.hist_ply = 0;
        self.castle_perm = 0;
        self.key = 0;
    }

    pub fn set_from_fen(&mut self, fen: &str) {
        assert!(fen.is_ascii());

        let mut rank = Rank::Rank8 as u8;
        let mut file = File::FileA as u8;

        self.reset();

        let fen_chars = fen.as_bytes();
        let split_idx = fen_chars.iter().position(|&c| c == b' ').unwrap();
        let (board_part, info_part) = fen_chars.split_at(split_idx);

        for &c in board_part {
            let mut count = 1;
            let piece;
            match c {
                b'P' => piece = Piece::WP as u8,
                b'R' => piece = Piece::WR as u8,
                b'N' => piece = Piece::WN as u8,
                b'B' => piece = Piece::WB as u8,
                b'Q' => piece = Piece::WQ as u8,
                b'K' => piece = Piece::WK as u8,
                b'p' => piece = Piece::BP as u8,
                b'r' => piece = Piece::BR as u8,
                b'n' => piece = Piece::BN as u8,
                b'b' => piece = Piece::BB as u8,
                b'q' => piece = Piece::BQ as u8,
                b'k' => piece = Piece::BK as u8,
                b'1'..=b'8' => {
                    piece = Piece::Empty as u8;
                    count = c - b'0';
                }
                b'/' => {
                    rank -= 1;
                    file = File::FileA as u8;
                    continue;
                }
                c => {
                    panic!(
                        "FEN string is invalid, got unexpected character: \"{}\"",
                        c as char
                    );
                }
            }

            for _ in 0..count {
                let sq64 = rank * 8 + file;
                let sq120 = SQ64_TO_SQ120[sq64 as usize];
                if piece != Piece::Empty as u8 {
                    self.pieces[sq120 as usize] = piece;
                }
                file += 1;
            }
        }

        let mut info_parts = info_part[1..].split(|&c| c == b' ');

        self.parse_side(info_parts.next());

        self.parse_castling(info_parts.next());

        self.parse_ep(info_parts.next());

        self.parse_halfmove(info_parts.next());

        self.parse_fullmove(info_parts.next());

        self.key = self.generate_pos_key();
    }

    fn parse_side(&mut self, side_part: Option<&[u8]>) {
        self.side = match side_part {
            None => panic!("FEN string is invalid, expected side part."),
            Some([b'w']) => Colour::White as u8,
            Some([b'b']) => Colour::Black as u8,
            Some(other) => panic!(
                "FEN string is invalid, expected side to be 'w' or 'b', got \"{}\"",
                std::str::from_utf8(other).unwrap()
            ),
        };
    }

    fn parse_castling(&mut self, castling_part: Option<&[u8]>) {
        match castling_part {
            None => panic!("FEN string is invalid, expected castling part."),
            Some([b'-']) => self.castle_perm = 0,
            Some(castling) => {
                for &c in castling {
                    match c {
                        b'K' => self.castle_perm |= Castling::WK as u8,
                        b'Q' => self.castle_perm |= Castling::WQ as u8,
                        b'k' => self.castle_perm |= Castling::BK as u8,
                        b'q' => self.castle_perm |= Castling::BQ as u8,
                        _ => panic!("FEN string is invalid, expected castling part to be of the form 'KQkq', got \"{}\"", castling.iter().map(|&c| c as char).collect::<String>()),
                    }
                }
            }
        }
    }

    fn parse_ep(&mut self, ep_part: Option<&[u8]>) {
        match ep_part {
            None => panic!("FEN string is invalid, expected en passant part."),
            Some([b'-']) => self.ep_sq = Square120::NoSquare as u8,
            Some(ep_sq) => {
                assert!(ep_sq.len() == 2, "FEN string is invalid, expected en passant part to be of the form 'a1', got \"{}\"", ep_sq.iter().map(|&c| c as char).collect::<String>());
                let file = ep_sq[0] as u8 - b'a';
                let rank = ep_sq[1] as u8 - b'1';
                assert!(file >= File::FileA as u8 && file <= File::FileH as u8);
                assert!(rank >= Rank::Rank1 as u8 && rank <= Rank::Rank8 as u8);
                self.ep_sq = filerank_to_square(file, rank);
            }
        }
    }

    fn parse_halfmove(&mut self, halfmove_part: Option<&[u8]>) {
        match halfmove_part {
            None => panic!("FEN string is invalid, expected halfmove clock part."),
            Some(halfmove_clock) => {
                self.fifty_move_counter = std::str::from_utf8(halfmove_clock)
                    .expect("FEN string is invalid, expected halfmove clock part to be valid UTF-8")
                    .parse::<u8>()
                    .expect("FEN string is invalid, expected halfmove clock part to be a number");
            }
        }
    }

    fn parse_fullmove(&mut self, fullmove_part: Option<&[u8]>) {
        match fullmove_part {
            None => panic!("FEN string is invalid, expected fullmove number part."),
            Some(fullmove_number) => {
                self.ply = std::str::from_utf8(fullmove_number)
                    .expect(
                        "FEN string is invalid, expected fullmove number part to be valid UTF-8",
                    )
                    .parse::<usize>()
                    .expect("FEN string is invalid, expected fullmove number part to be a number")
                    * 2;
                if self.side == Colour::Black as u8 {
                    self.ply += 1;
                }
            }
        }
    }

    fn update_list_material(&mut self) {
        for index in 0..BOARD_N_SQUARES {
            let sq = index;
            let piece = self.pieces[index];
            if piece != Square120::OffBoard as u8 && piece != Piece::Empty as u8 {
                let colour = PIECE_COL[piece as usize];

                if PIECE_BIG[piece as usize] {
                    self.big_piece_counts[colour as usize] += 1;
                }
                if PIECE_MIN[piece as usize] {
                    self.minor_piece_counts[colour as usize] += 1;
                }
                if PIECE_MAJ[piece as usize] {
                    self.major_piece_counts[colour as usize] += 1;
                }

                let pval: u16 = PIECE_VAL[piece as usize].try_into().unwrap();
                self.material[colour as usize] += pval;

                self.p_list[piece as usize][self.piece_counts[piece as usize] as usize] = sq.try_into().unwrap();
                self.piece_counts[piece as usize] += 1;

                if piece == Piece::WK as u8 || piece == Piece::BK as u8 {
                    self.king_sq[colour as usize] = sq.try_into().unwrap();
                }
            }
        }
    }
}

impl Display for Board {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        const PIECE_CHAR: [u8; 13] = *b".PNBRQKpnbrqk";
        const SIDE_CHAR: [u8; 3] = *b"wb-";
        const RANK_CHAR: [u8; 8] = *b"12345678";
        const FILE_CHAR: [u8; 8] = *b"abcdefgh";

        writeln!(f, "Game Board:")?;

        for rank in ((Rank::Rank1 as u8)..=(Rank::Rank8 as u8)).rev() {
            write!(f, "{} ", rank + 1)?;
            for file in (File::FileA as u8)..=(File::FileH as u8) {
                let sq = filerank_to_square(file, rank);
                let piece = self.pieces[sq as usize];
                write!(f, "{} ", PIECE_CHAR[piece as usize] as char)?;
            }
            writeln!(f)?;
        }

        writeln!(f, "  a b c d e f g h")?;
        writeln!(f, "side: {}", SIDE_CHAR[self.side as usize] as char)?;

        Ok(())
    }
}

impl Debug for Board {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self)?;
        writeln!(f, "ep-square: {}", self.ep_sq)?;
        writeln!(f, "castling: {:b}", self.castle_perm)?;
        writeln!(f, "fifty-move-counter: {}", self.fifty_move_counter)?;
        writeln!(f, "ply: {}", self.ply)?;
        writeln!(f, "hash: {:x}", self.key)?;
        Ok(())
    }
}
