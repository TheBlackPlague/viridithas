use serde_json::Value;

use crate::{board::{Board, movegen::BitLoop}, definitions::{flip_rank, BLACK, WHITE, PAWN, KING}, lookups::{piece_from_cotype, SQUARE_NAMES}};

use self::accumulator::Accumulator;

pub mod convert;
mod accumulator;

const INPUT: usize = 768;
const HIDDEN: usize = 256;
const CR_MIN: i16 = 0;
const CR_MAX: i16 = 255;
const SCALE: i32 = 400;

const QA: i32 = 255;
const QB: i32 = 64;
const QAB: i32 = QA * QB;

const ACC_STACK_SIZE: usize = 256;

pub const ACTIVATE: bool = true;
pub const DEACTIVATE: bool = false;

// read in bytes from files and transmute them into u16s.
// SAFETY: alignment to u16 is guaranteed because transmute() is a copy operation.
pub static NNUE: NNUEParams = NNUEParams {
    feature_weights: unsafe { std::mem::transmute(*include_bytes!("../../nnue/feature_weights.bin")) },
    flipped_weights: unsafe { std::mem::transmute(*include_bytes!("../../nnue/flipped_weights.bin")) },
    feature_bias: unsafe { std::mem::transmute(*include_bytes!("../../nnue/feature_bias.bin")) },
    output_weights: unsafe { std::mem::transmute(*include_bytes!("../../nnue/output_weights.bin")) },
    output_bias: unsafe { std::mem::transmute(*include_bytes!("../../nnue/output_bias.bin")) },
};

pub struct NNUEParams {
    pub feature_weights: [i16; INPUT * HIDDEN],
    pub flipped_weights: [i16; INPUT * HIDDEN],
    pub feature_bias: [i16; HIDDEN],
    pub output_weights: [i16; HIDDEN * 2],
    pub output_bias: i16,
}

impl NNUEParams {
    pub fn from_json(path: &str) -> Box<Self> {
        #![allow(clippy::cast_possible_truncation)]
        fn weight(weight_relation: &Value, weight_array: &mut [i16], stride: usize, k: i32, flip: bool) {
            for (i, output) in weight_relation.as_array().unwrap().iter().enumerate() {
                for (j, weight) in output.as_array().unwrap().iter().enumerate() {
                    let index = if flip {
                        (j * stride + i) as usize
                    } else {
                        (i * stride + j) as usize
                    };
                    let value = weight.as_f64().unwrap();
                    weight_array[index] = (value * f64::from(k)) as i16;
                }
            }
        }

        fn bias(bias_relation: &Value, bias_array: &mut [i16], k: i32) {
            for (i, bias) in bias_relation.as_array().unwrap().iter().enumerate() {
                let value = bias.as_f64().unwrap();
                bias_array[i] = (value * f64::from(k)) as i16;
            }
        }

        let mut out = Box::new(Self {
            feature_weights: [0; INPUT * HIDDEN],
            flipped_weights: [0; INPUT * HIDDEN],
            feature_bias: [0; HIDDEN],
            output_weights: [0; HIDDEN * 2],
            output_bias: 0,
        });

        let file = std::fs::read_to_string(path).unwrap();
        let json: Value = serde_json::from_str(&file).unwrap();

        for property in json.as_object().unwrap() {
            match property.0.as_str() {
                "ft.weight" => {
                    weight(property.1, &mut out.feature_weights, INPUT, QA, false);
                    weight(property.1, &mut out.flipped_weights, HIDDEN, QA, true);
                    println!("feature weights loaded");
                }
                "ft.bias" => {
                    bias(property.1, &mut out.feature_bias, QA);
                    println!("feature bias loaded");
                }
                "out.weight" => {
                    weight(property.1, &mut out.output_weights, HIDDEN * 2, QB, false);
                    println!("output weights loaded");
                }
                "out.bias" => {
                    let mut temparr = [0];
                    bias(property.1, &mut temparr, QAB);
                    out.output_bias = temparr[0];
                    println!("output bias loaded");
                }
                _ => {}
            }
        }

        println!("nnue loaded");

        out
    }

    pub fn to_bytes(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();

        let (head, feature_weights, tail) = unsafe { self.feature_weights.align_to::<u8>() };
        assert!(head.is_empty() && tail.is_empty());
        let (head, flipped_weights, tail) = unsafe { self.flipped_weights.align_to::<u8>() };
        assert!(head.is_empty() && tail.is_empty());
        let (head, feature_bias, tail) = unsafe { self.feature_bias.align_to::<u8>() };
        assert!(head.is_empty() && tail.is_empty());
        let (head, output_weights, tail) = unsafe { self.output_weights.align_to::<u8>() };
        assert!(head.is_empty() && tail.is_empty());
        let ob = [self.output_bias];
        let (head, output_bias, tail) = unsafe { ob.align_to::<u8>() };
        assert!(head.is_empty() && tail.is_empty());

        out.push(feature_weights.to_vec());
        out.push(flipped_weights.to_vec());
        out.push(feature_bias.to_vec());
        out.push(output_weights.to_vec());
        out.push(output_bias.to_vec());

        out
    }
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug)]
pub struct NNUEState {
    pub white_pov: [i16; INPUT],
    pub black_pov: [i16; INPUT],

    pub accumulators: [Accumulator<HIDDEN>; ACC_STACK_SIZE],
    pub current_acc: usize,
}

impl NNUEState {
    pub const fn new() -> Self {
        Self {
            white_pov: [0; INPUT],
            black_pov: [0; INPUT],
            accumulators: [Accumulator::new(); ACC_STACK_SIZE],
            current_acc: 0,
        }
    }

    pub fn push_acc(&mut self) {
        self.accumulators[self.current_acc + 1] = self.accumulators[self.current_acc];
        self.current_acc += 1;
    }

    pub fn pop_acc(&mut self) {
        self.current_acc -= 1;
    }

    pub fn refresh_acc(&mut self, board: &Board) {
        self.current_acc = 0;

        self.white_pov.fill(0);
        self.black_pov.fill(0);

        self.accumulators[self.current_acc].init(&NNUE.feature_bias);

        for colour in [WHITE, BLACK] {
            for piece_type in PAWN..=KING {
                let piece = piece_type + 6 * colour;
                let piece_bb = board.pieces.piece_bb(piece);

                for sq in BitLoop::new(piece_bb) {
                    self.efficiently_update_manual::<ACTIVATE>(piece_type, colour, sq);
                }
            }
        }
    }

    pub fn efficiently_update_from_move(&mut self, piece: u8, colour: u8, from: u8, to: u8) {
        #![allow(clippy::cast_possible_truncation)]
        const COLOUR_STRIDE: usize = 64 * 6;
        const PIECE_STRIDE: usize = 64;

        let piece = piece as usize - 1; // shift into correct range.
        let colour = colour as usize;

        let white_idx_from = colour * COLOUR_STRIDE + piece * PIECE_STRIDE + from as usize;
        let black_idx_from = (1 ^ colour) * COLOUR_STRIDE + piece * PIECE_STRIDE + flip_rank(from) as usize;
        let white_idx_to = colour * COLOUR_STRIDE + piece * PIECE_STRIDE + to as usize;
        let black_idx_to = (1 ^ colour) * COLOUR_STRIDE + piece * PIECE_STRIDE + flip_rank(to) as usize;

        debug_assert_eq!(self.white_pov[white_idx_from], 1, "piece: {}, from: {}, to: {}", crate::lookups::piece_name(piece_from_cotype(colour as u8, piece as u8 + 1)).unwrap(), SQUARE_NAMES[from as usize], SQUARE_NAMES[to as usize]);
        debug_assert_eq!(self.black_pov[black_idx_from], 1, "piece: {}, from: {}, to: {}", crate::lookups::piece_name(piece_from_cotype(colour as u8, piece as u8 + 1)).unwrap(), SQUARE_NAMES[from as usize], SQUARE_NAMES[to as usize]);
        debug_assert_eq!(self.white_pov[white_idx_to], 0, "piece: {}, from: {}, to: {}", crate::lookups::piece_name(piece_from_cotype(colour as u8, piece as u8 + 1)).unwrap(), SQUARE_NAMES[from as usize], SQUARE_NAMES[to as usize]);
        debug_assert_eq!(self.black_pov[black_idx_to], 0, "piece: {}, from: {}, to: {}", crate::lookups::piece_name(piece_from_cotype(colour as u8, piece as u8 + 1)).unwrap(), SQUARE_NAMES[from as usize], SQUARE_NAMES[to as usize]);
        self.white_pov[white_idx_from] = 0;
        self.black_pov[black_idx_from] = 0;
        self.white_pov[white_idx_to] = 1;
        self.black_pov[black_idx_to] = 1;

        let acc = &mut self.accumulators[self.current_acc];

        subtract_and_add_to_all(
            &mut acc.white, 
            &NNUE.flipped_weights,
            white_idx_from * HIDDEN,
            white_idx_to * HIDDEN,
        );
        subtract_and_add_to_all(
            &mut acc.black, 
            &NNUE.flipped_weights,
            black_idx_from * HIDDEN,
            black_idx_to * HIDDEN,
        );
    }

    pub fn efficiently_update_manual<const IS_ACTIVATE: bool>(&mut self, piece: u8, colour: u8, sq: u8) {
        #![allow(clippy::cast_possible_truncation)]
        const COLOUR_STRIDE: usize = 64 * 6;
        const PIECE_STRIDE: usize = 64;

        let sq = sq;
        let piece = piece as usize - 1; // shift into correct range.
        let colour = colour as usize;
        
        let white_idx = colour * COLOUR_STRIDE + piece * PIECE_STRIDE + sq as usize;
        let black_idx = (1 ^ colour) * COLOUR_STRIDE + piece * PIECE_STRIDE + flip_rank(sq) as usize;

        let acc = &mut self.accumulators[self.current_acc];

        if IS_ACTIVATE {
            debug_assert!(self.white_pov[white_idx] == 0, "piece: {}, sq: {}", crate::lookups::piece_name(piece_from_cotype(colour as u8, piece as u8 + 1)).unwrap(), SQUARE_NAMES[sq as usize]);
            debug_assert!(self.black_pov[black_idx] == 0, "piece: {}, sq: {}", crate::lookups::piece_name(piece_from_cotype(colour as u8, piece as u8 + 1)).unwrap(), SQUARE_NAMES[sq as usize]);
            self.white_pov[white_idx] = 1;
            self.black_pov[black_idx] = 1;
            add_to_all(&mut acc.white, &NNUE.flipped_weights, white_idx * HIDDEN);
            add_to_all(&mut acc.black, &NNUE.flipped_weights, black_idx * HIDDEN);
        } else {
            debug_assert!(self.white_pov[white_idx] == 1, "piece: {}, sq: {}", crate::lookups::piece_name(piece_from_cotype(colour as u8, piece as u8 + 1)).unwrap(), SQUARE_NAMES[sq as usize]);
            debug_assert!(self.black_pov[black_idx] == 1, "piece: {}, sq: {}", crate::lookups::piece_name(piece_from_cotype(colour as u8, piece as u8 + 1)).unwrap(), SQUARE_NAMES[sq as usize]);
            self.white_pov[white_idx] = 0;
            self.black_pov[black_idx] = 0;
            sub_from_all(&mut acc.white, &NNUE.flipped_weights, white_idx * HIDDEN);
            sub_from_all(&mut acc.black, &NNUE.flipped_weights, black_idx * HIDDEN);
        }
    }

    pub fn update_pov_move(&mut self, piece: u8, colour: u8, from: u8, to: u8) {
        const COLOUR_STRIDE: usize = 64 * 6;
        const PIECE_STRIDE: usize = 64;

        let piece = piece as usize - 1; // shift into correct range.
        let colour = colour as usize;

        let white_idx_from = colour * COLOUR_STRIDE + piece * PIECE_STRIDE + from as usize;
        let black_idx_from = (1 ^ colour) * COLOUR_STRIDE + piece * PIECE_STRIDE + flip_rank(from) as usize;
        let white_idx_to = colour * COLOUR_STRIDE + piece * PIECE_STRIDE + to as usize;
        let black_idx_to = (1 ^ colour) * COLOUR_STRIDE + piece * PIECE_STRIDE + flip_rank(to) as usize;

        self.white_pov[white_idx_from] = 0;
        self.black_pov[black_idx_from] = 0;
        self.white_pov[white_idx_to] = 1;
        self.black_pov[black_idx_to] = 1;
    }

    pub fn update_pov_manual<const IS_ACTIVATE: bool>(&mut self, piece: u8, colour: u8, sq: u8) {
        const COLOUR_STRIDE: usize = 64 * 6;
        const PIECE_STRIDE: usize = 64;

        let sq = sq;
        let piece = piece as usize - 1; // shift into correct range.
        let colour = colour as usize;
        
        let white_idx = colour * COLOUR_STRIDE + piece * PIECE_STRIDE + sq as usize;
        let black_idx = (1 ^ colour) * COLOUR_STRIDE + piece * PIECE_STRIDE + flip_rank(sq) as usize;

        if IS_ACTIVATE {
            self.white_pov[white_idx] = 1;
            self.black_pov[black_idx] = 1;
        } else {
            self.white_pov[white_idx] = 0;
            self.black_pov[black_idx] = 0;
        }
    }

    pub fn evaluate(&self, stm: u8) -> i32 {
        let acc = &self.accumulators[self.current_acc];
        
        let output = if stm == WHITE {
            clipped_relu_flatten_and_forward::<CR_MIN, CR_MAX, HIDDEN, { HIDDEN * 2 }>(
                &acc.white,
                &acc.black,
                &NNUE.output_weights,
            )
        } else {
            clipped_relu_flatten_and_forward::<CR_MIN, CR_MAX, HIDDEN, { HIDDEN * 2 }>(
                &acc.black,
                &acc.white,
                &NNUE.output_weights,
            )
        };

        (output + i32::from(NNUE.output_bias)) * SCALE / QAB
    }

    pub fn active_features(&self) -> impl Iterator<Item = usize> + '_ {
        self.white_pov
            .iter()
            .enumerate()
            .filter(|(_, &x)| x == 1)
            .map(|(i, _)| i)
    }

    pub const fn feature_loc_to_parts(loc: usize) -> (u8, u8, u8) {
        #![allow(clippy::cast_possible_truncation)]
        const COLOUR_STRIDE: usize = 64 * 6;
        const PIECE_STRIDE: usize = 64;
        let colour = (loc / COLOUR_STRIDE) as u8;
        let rem = loc % COLOUR_STRIDE;
        let piece = (rem / PIECE_STRIDE) as u8;
        let sq = (rem % PIECE_STRIDE) as u8;
        (colour, piece, sq)
    }
}

fn subtract_and_add_to_all<const SIZE: usize, const WEIGHTS: usize>(
    input: &mut [i16; SIZE],
    delta: &[i16; WEIGHTS],
    offset_sub: usize,
    offset_add: usize,
) {
    for ((ds, da), i) in delta[offset_sub..offset_sub + SIZE]
        .iter()
        .zip(delta[offset_add..offset_add + SIZE].iter())
        .zip(input.iter_mut())
    {
        *i = *i - *ds + *da;
    }
}

fn add_to_all<const SIZE: usize, const WEIGHTS: usize>(
    input: &mut [i16; SIZE],
    delta: &[i16; WEIGHTS],
    offset_add: usize,
) {
    for (i, d) in input.iter_mut().zip(&delta[offset_add..]) {
        *i += *d;
    }
}

fn sub_from_all<const SIZE: usize, const WEIGHTS: usize>(
    input: &mut [i16; SIZE],
    delta: &[i16; WEIGHTS],
    offset_sub: usize,
) {
    for (i, d) in input.iter_mut().zip(&delta[offset_sub..]) {
        *i -= *d;
    }
}

pub fn clipped_relu_flatten_and_forward<
    const MIN: i16,
    const MAX: i16,
    const SIZE: usize,
    const WEIGHTS: usize,
>(
    input_us: &[i16; SIZE],
    input_them: &[i16; SIZE],
    weights: &[i16; WEIGHTS],
) -> i32 {
    debug_assert_eq!(SIZE * 2, WEIGHTS);
    let mut sum: i32 = 0;
    for (&i, &w) in input_us.iter().zip(weights) {
        sum += i32::from(i.clamp(MIN, MAX)) * i32::from(w);
    }
    for (&i, &w) in input_them.iter().zip(&weights[SIZE..]) {
        sum += i32::from(i.clamp(MIN, MAX)) * i32::from(w);
    }
    sum
}

pub fn convert_json_to_binary(json_path: &str, output_path: &str) {
    let nnue = NNUEParams::from_json(json_path);
    let bytes = nnue.to_bytes();
    std::fs::create_dir(output_path).unwrap();
    for (fname, byte_vector) in [
        "feature_weights", 
        "flipped_weights", 
        "feature_bias",
        "output_weights",
        "output_bias",
    ].into_iter().zip(&bytes) {
        let mut f = std::fs::File::create(format!("{}/{}.bin", output_path, fname)).unwrap();
        std::io::Write::write_all(&mut f, byte_vector).unwrap();
    }
}

mod tests {
    #[test]
    fn pov_preserved() {
        crate::magic::initialise();
        let mut board = crate::board::Board::default();
        board.alloc_tables();
        let mut t = crate::threadlocal::ThreadData::new();
        let mut ml = crate::board::movegen::MoveList::new();
        board.generate_moves(&mut ml);
        t.nnue.refresh_acc(&board);
        let initial_white = t.nnue.white_pov;
        let initial_black = t.nnue.black_pov;
        for &m in ml.iter() {
            if !board.make_move_nnue(m, &mut t) {
                continue;
            }
            board.unmake_move_nnue(&mut t);
            assert_eq!(initial_white, t.nnue.white_pov);
            assert_eq!(initial_black, t.nnue.black_pov);
        }
    }

    #[test]
    fn pov_preserved_ep() {
        crate::magic::initialise();
        let mut board = crate::board::Board::from_fen("rnbqkbnr/1pp1ppp1/p7/2PpP2p/8/8/PP1P1PPP/RNBQKBNR w KQkq d6 0 5").unwrap();
        board.alloc_tables();
        let mut t = crate::threadlocal::ThreadData::new();
        let mut ml = crate::board::movegen::MoveList::new();
        board.generate_moves(&mut ml);
        t.nnue.refresh_acc(&board);
        let initial_white = t.nnue.white_pov;
        let initial_black = t.nnue.black_pov;
        for &m in ml.iter() {
            if !board.make_move_nnue(m, &mut t) {
                continue;
            }
            board.unmake_move_nnue(&mut t);
            assert_eq!(initial_white, t.nnue.white_pov);
            assert_eq!(initial_black, t.nnue.black_pov);
        }
    }

    #[test]
    fn pov_preserved_castling() {
        crate::magic::initialise();
        let mut board = crate::board::Board::from_fen("rnbqkbnr/1pp1p3/p4pp1/2PpP2p/8/3B1N2/PP1P1PPP/RNBQK2R w KQkq - 0 7").unwrap();
        board.alloc_tables();
        let mut t = crate::threadlocal::ThreadData::new();
        let mut ml = crate::board::movegen::MoveList::new();
        board.generate_moves(&mut ml);
        t.nnue.refresh_acc(&board);
        let initial_white = t.nnue.white_pov;
        let initial_black = t.nnue.black_pov;
        for &m in ml.iter() {
            if !board.make_move_nnue(m, &mut t) {
                continue;
            }
            board.unmake_move_nnue(&mut t);
            assert_eq!(initial_white, t.nnue.white_pov);
            assert_eq!(initial_black, t.nnue.black_pov);
        }
    }

    #[test]
    fn pov_preserved_promo() {
        crate::magic::initialise();
        let mut board = crate::board::Board::from_fen("rnbqk2r/1pp1p1P1/p4np1/2Pp3p/8/3B1N2/PP1P1PPP/RNBQK2R w KQkq - 1 9").unwrap();
        board.alloc_tables();
        let mut t = crate::threadlocal::ThreadData::new();
        let mut ml = crate::board::movegen::MoveList::new();
        board.generate_moves(&mut ml);
        t.nnue.refresh_acc(&board);
        let initial_white = t.nnue.white_pov;
        let initial_black = t.nnue.black_pov;
        for &m in ml.iter() {
            println!();
            if !board.make_move_nnue(m, &mut t) {
                continue;
            }
            println!("{m}");
            board.unmake_move_nnue(&mut t);
            for i in 0..768 {
                if initial_white[i] != t.nnue.white_pov[i] {
                    eprintln!("{i}: {} != {}", initial_white[i], t.nnue.white_pov[i]);
                }
                if initial_black[i] != t.nnue.black_pov[i] {
                    eprintln!("{i}: {} != {}", initial_black[i], t.nnue.black_pov[i]);
                }
            }
            assert_eq!(initial_white, t.nnue.white_pov);
            assert_eq!(initial_black, t.nnue.black_pov);
        }
    }
}