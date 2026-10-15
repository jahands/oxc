use oxc_span::Span;

use crate::{
    lanes::Lanes,
    token::{SPAN_SENTINELS, tk},
};

use crate::pipeline::platform::{Features, Platform, platform_select};

#[cfg(target_arch = "x86_64")]
mod avx2;
#[cfg(target_arch = "x86_64")]
use avx2::{
    build_spans_avx2, compress_avx2, compress_blocks_avx2, init_pair_luts_avx2, lanes_post_avx2,
};

mod generic;
use generic::{build_spans_generic, compress_blocks_generic, compress_generic, lanes_post_generic};

mod common;

const STAGE_BLOCKS: usize = 32;
pub const STAGE_CAP: usize = STAGE_BLOCKS * 64 + 128;

#[inline(always)]
pub unsafe fn compress(
    src: &[u8],
    n: usize,
    nb: usize,
    st: *const u64,
    kind: *const u8,
    stage_pos: *mut u32,
    stage_kind: *mut u8,
    out_kinds: *mut u8,
    out_spans: *mut Span,
    lanes: &mut Lanes,
    feat: impl Features,
) -> usize {
    platform_select!(feat, {
        Avx2(feat) => {
            compress_avx2(src, n, nb, st, kind, stage_pos, stage_kind, out_kinds, out_spans, lanes, feat)
        }
        Neon(feat) => {
            compress_generic(src, n, nb, st, kind, stage_pos, stage_kind, out_kinds, out_spans, lanes, feat)
        }
        Generic(feat) => {
            compress_generic(src, n, nb, st, kind, stage_pos, stage_kind, out_kinds, out_spans, lanes, feat)
        }
    })
}

/// Body of [`compress`].
///
/// `#[inline(always)]` so it gets compiled as part of its caller - `compress_avx2` or `compress_generic`.
/// `compress_avx2` is compiled with AVX2 enabled, so the AVX2 functions which this calls can be inlined into it.
#[inline(always)]
unsafe fn compress_impl(
    src: &[u8],
    n: usize,
    nb: usize,
    st: *const u64,
    kind: *const u8,
    stage_pos: *mut u32,
    stage_kind: *mut u8,
    out_kinds: *mut u8,
    out_spans: *mut Span,
    lanes: &mut Lanes,
    feat: impl Features,
) -> usize {
    let mut c = 0usize;
    let mut w = 0usize;
    let mut b = 0usize;
    while b < nb {
        let b1 = (b + STAGE_BLOCKS).min(nb);
        c += compress_blocks(st, kind, b, b1, stage_pos.add(c), stage_kind.add(c), feat);
        b = b1;
        if c > 1 {
            w +=
                build_spans(stage_kind, stage_pos, c - 1, out_spans.add(w), out_kinds.add(w), feat);
            *stage_pos = *stage_pos.add(c - 1);
            *stage_kind = *stage_kind.add(c - 1);
            c = 1;
        }
    }
    if c == 1 {
        *stage_pos.add(1) = n as u32;
        w += build_spans(stage_kind, stage_pos, 1, out_spans.add(w), out_kinds.add(w), feat);
    }
    write_sentinels(n as u32, out_spans.add(w), out_kinds.add(w));
    lanes_post(src, out_kinds, out_spans, w, n as u32, lanes, feat);
    w
}

pub unsafe fn write_sentinels(n: u32, spans: *mut Span, sig_kinds: *mut u8) {
    let eof = u64::from(n) | (u64::from(n) << 32);
    for s in 0..SPAN_SENTINELS {
        *spans.cast::<u64>().add(s) = eof;
        *sig_kinds.add(s) = tk!(Eof);
    }
}

/// Initialize the global `PairLuts` instance.
///
/// This function must be called before calling [`compress`].
///
/// `PairLuts` is only used by AVX2 implementation, so this function is a no-op on other platforms.
pub(super) fn init_pair_luts(platform: Platform) {
    platform_select!(platform, {
        Avx2(feat) => init_pair_luts_avx2(feat),
        Neon(_) => {}
        Generic(_) => {}
    });
}

#[inline(always)]
unsafe fn compress_blocks(
    st: *const u64,
    kind: *const u8,
    b0: usize,
    b1: usize,
    starts: *mut u32,
    kinds: *mut u8,
    feat: impl Features,
) -> usize {
    platform_select!(feat, {
        Avx2(feat) => compress_blocks_avx2(st, kind, b0, b1, starts, kinds, feat),
        Neon(_) => compress_blocks_generic(st, kind, b0, b1, starts, kinds),
        Generic(_) => compress_blocks_generic(st, kind, b0, b1, starts, kinds),
    })
}

#[inline(always)]
unsafe fn build_spans(
    stage_kind: *const u8,
    stage_pos: *const u32,
    m: usize,
    spans: *mut Span,
    sig_kinds: *mut u8,
    feat: impl Features,
) -> usize {
    platform_select!(feat, {
        Avx2(feat) => build_spans_avx2(stage_kind, stage_pos, m, spans, sig_kinds, feat),
        Neon(_) => build_spans_generic(stage_kind, stage_pos, m, spans, sig_kinds),
        Generic(_) => build_spans_generic(stage_kind, stage_pos, m, spans, sig_kinds),
    })
}

#[inline(always)]
unsafe fn lanes_post(
    src: &[u8],
    out_kinds: *const u8,
    out_spans: *const Span,
    m: usize,
    nn: u32,
    lanes: &mut Lanes,
    feat: impl Features,
) {
    platform_select!(feat, {
        Avx2(feat) => lanes_post_avx2(src, out_kinds, out_spans, m, nn, lanes, feat),
        Neon(_) => lanes_post_generic(src, out_kinds, out_spans, m, nn, lanes),
        Generic(_) => lanes_post_generic(src, out_kinds, out_spans, m, nn, lanes),
    });
}
