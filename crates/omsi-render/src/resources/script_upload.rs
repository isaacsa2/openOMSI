//! The pictures the vehicles' scripts draw (`[scripttexture]`: matrix displays, IBIS screens)
//! go up when a script lets go of them. A script that redraws a display every frame often
//! draws the picture it drew the frame before - the same destination, the same clock minute -
//! and the upload (the copy, and for a display with mip levels the whole chain made again and
//! the materials pointed at it) bought nothing.
//!
//! [`Renderer::update_script_texture`] knows what it last put into a texture and leaves it
//! alone when the picture is the same. What it knows is kept in the [`Scene`] beside the
//! texture and says which texture it was (`GpuTexture::gen`, new with every texture made or
//! replaced, so a slot given out again never matches what the one before it held).

use super::textures::GpuTexture;
use crate::{Renderer, Scene, TextureId};
use omsi_texture::Image;

/// What [`Renderer::update_script_texture`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptWrite {
    /// The texture held this picture already: nothing was written.
    Unchanged,
    /// The picture went up into the texture.
    Written,
    /// The texture was made again (another size, another number of levels): the materials
    /// that use it have to be pointed at it (`Renderer::rebind_textures`).
    Replaced,
}

/// A digest of a picture and of how it is shown: its size, whether it has mip levels, its
/// bytes. Thirty-two bytes at a time in four lanes that do not wait for each other (one lane
/// is bound by the multiplication it waits for, a fifth of what memory gives). Every step
/// is one that cannot give the same result for two values of the word it takes, so a picture
/// that differs in one word - a pixel or two - never has the same digest as the one before it.
pub(crate) fn digest(width: u32, height: u32, mips: bool, rgba: &[u8]) -> u64 {
    const K: u64 = 0x9E37_79B9_7F4A_7C15;
    let step = |h: u64, w: u64| (h.rotate_left(5) ^ w).wrapping_mul(K);
    let word = |b: &[u8]| u64::from_le_bytes(b.try_into().unwrap());
    let seed = ((width as u64) << 33 ^ (height as u64) << 1 ^ mips as u64).wrapping_mul(K) ^ rgba.len() as u64;
    let mut lane = [seed, seed ^ 0x1111_1111_1111_1111, seed ^ 0x2222_2222_2222_2222, seed ^ 0x3333_3333_3333_3333];
    let mut blocks = rgba.chunks_exact(32);
    for b in &mut blocks {
        for (i, l) in lane.iter_mut().enumerate() {
            *l = step(*l, word(&b[i * 8..i * 8 + 8]));
        }
    }
    let mut h = lane.iter().fold(0u64, |h, &l| step(h, l));
    let mut words = blocks.remainder().chunks_exact(8);
    for w in &mut words {
        h = step(h, word(w));
    }
    let mut tail = [0u8; 8];
    tail[..words.remainder().len()].copy_from_slice(words.remainder());
    h = step(h, u64::from_le_bytes(tail));
    h ^ (h >> 32)
}

impl Renderer {
    /// Put the picture a script drew into texture `id` - unless the texture holds it already.
    /// `mips`: the display is shown with mip levels (`STFilter`).
    ///
    /// Every script texture goes up through here; nothing else writes into one, so what was
    /// last written is what the texture holds.
    pub fn update_script_texture(&self, scene: &mut Scene, id: TextureId, img: &Image, mips: bool) -> ScriptWrite {
        let sum = digest(img.width, img.height, mips, &img.rgba);
        let gen = |t: Option<&GpuTexture>| t.map(|t| t.gen);
        if let Some(g) = gen(scene.textures.get(id)) {
            if scene.script_sent.get(&id) == Some(&(g, sum)) {
                return ScriptWrite::Unchanged;
            }
        }
        let replaced = if mips {
            self.update_texture_mips(scene, id, img)
        } else {
            self.update_texture(scene, id, img);
            false
        };
        // (what is known after the write: a texture made again has a gen of its own)
        match gen(scene.textures.get(id)) {
            Some(g) => {
                scene.script_sent.insert(id, (g, sum));
            }
            None => {
                scene.script_sent.remove(&id);
            }
        }
        if replaced {
            ScriptWrite::Replaced
        } else {
            ScriptWrite::Written
        }
    }
}

#[cfg(test)]
mod tests {
    use super::digest;

    #[test]
    fn a_picture_that_differs_has_another_digest() {
        let a = vec![7u8; 64 * 32 * 4];
        assert_eq!(digest(64, 32, false, &a), digest(64, 32, false, &a.clone()));
        // one byte, in every place a word can be, and in the tail that is not a whole word
        for i in [0usize, 1, 7, 8, 16, 24, 31, 32, 40, 100, a.len() - 9, a.len() - 1] {
            let mut b = a.clone();
            b[i] ^= 1;
            assert_ne!(digest(64, 32, false, &a), digest(64, 32, false, &b), "byte {i}");
        }
        // the same bytes in another shape, or shown another way
        assert_ne!(digest(64, 32, false, &a), digest(32, 64, false, &a));
        assert_ne!(digest(64, 32, false, &a), digest(64, 32, true, &a));
        // a length that is not a multiple of eight
        let odd = vec![1u8; 13];
        let mut odd2 = odd.clone();
        odd2[12] = 2;
        assert_ne!(digest(1, 1, false, &odd), digest(1, 1, false, &odd2));
        // all zero (a cleared display) and all zero but one pixel
        let blank = vec![0u8; 256 * 64 * 4];
        let mut dot = blank.clone();
        dot[4 * (30 * 256 + 100) + 3] = 255;
        assert_ne!(digest(256, 64, true, &blank), digest(256, 64, true, &dot));
    }

    #[test]
    fn two_pictures_that_swap_two_words_differ() {
        // words in the same lane, in different lanes, and in the part after the blocks
        for (x, y, len) in [(0usize, 32usize, 96usize), (0, 8, 96), (8, 16, 96), (0, 64, 72), (64, 72, 80)] {
            let mut a = vec![0u8; len];
            let mut b = a.clone();
            a[x] = 1;
            a[y] = 2;
            b[x] = 2;
            b[y] = 1;
            assert_ne!(digest(4, 4, false, &a), digest(4, 4, false, &b), "{x} {y} {len}");
        }
    }

    #[test]
    fn every_length_has_a_digest_that_follows_each_byte() {
        // (lengths round the block, the word and the sub-word edges)
        for len in (0..=100usize).chain([255, 256, 257]) {
            let a = vec![5u8; len];
            for i in 0..len {
                let mut b = a.clone();
                b[i] = 6;
                assert_ne!(digest(2, 2, true, &a), digest(2, 2, true, &b), "len {len} byte {i}");
            }
        }
    }
}
