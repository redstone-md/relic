//! The launcher's two moving pictures: a mine shaft to fly down while a
//! server loads, and the player's skin on a turntable. Both are drawn on the
//! CPU a few screen pixels to the block, so they stay sharp at any scale
//! and need nothing from the graphics card.

use std::cell::RefCell;
use std::ops::{Add, Mul};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use gpui_kit::prelude::*;
use gpui_kit::{
    Bounds, Corners, Hsla, MouseMoveEvent, Pixels, RenderImage, Rgba, Window, canvas, div, px, size,
};
use image::{Frame, RgbaImage};
use relic_proto::Scene;

/// Block textures: sixteen 16 px tiles in a 4×4 sheet. See
/// `assets/CREDITS.md` for where they come from.
const SHEET: &[u8] = include_bytes!("../../assets/blocks.png");

mod tile {
    pub const STONE: usize = 0;
    pub const COBBLESTONE: usize = 1;
    pub const DEEPSLATE: usize = 2;
    pub const GOLD_ORE: usize = 3;
    pub const GLOWSTONE: usize = 4;
    pub const GRASS: usize = 5;
    pub const DIRT: usize = 6;
    pub const SAND: usize = 7;
    pub const SNOW: usize = 9;
    pub const LOG: usize = 10;
    pub const LEAVES: usize = 11;
    pub const DIAMOND_ORE: usize = 13;
    pub const ANDESITE: usize = 14;
    pub const GRAVEL: usize = 15;
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct V3(f32, f32, f32);

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        V3(self.0 + o.0, self.1 + o.1, self.2 + o.2)
    }
}

impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, k: f32) -> V3 {
        V3(self.0 * k, self.1 * k, self.2 * k)
    }
}

impl Mul<V3> for V3 {
    type Output = V3;
    fn mul(self, o: V3) -> V3 {
        V3(self.0 * o.0, self.1 * o.1, self.2 * o.2)
    }
}

impl V3 {
    fn mix(self, to: V3, t: f32) -> V3 {
        self * (1. - t) + to * t
    }

    fn max(self) -> f32 {
        self.0.max(self.1).max(self.2)
    }

    fn min(self) -> f32 {
        self.0.min(self.1).min(self.2)
    }

    fn map(self, f: impl Fn(f32) -> f32) -> V3 {
        V3(f(self.0), f(self.1), f(self.2))
    }

    /// As a screen pixel; the window wants blue first.
    fn bgra(self) -> [u8; 4] {
        let to = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
        [to(self.2), to(self.1), to(self.0), 255]
    }
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// A number from 0 to 1 that depends on nothing but its arguments.
fn chance(seed: u64, a: i64, b: i64, salt: u64) -> f32 {
    let mut x = seed
        .wrapping_add((a as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .wrapping_add((b as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9))
        .wrapping_add(salt.wrapping_mul(0x94d0_49bb_1331_11eb));
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// A stable number for a name, so every server gets its own shaft and
/// keeps it between runs.
pub fn seed_of(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The block sheet in linear light, one entry per texel.
fn sheet() -> &'static [V3] {
    static TEXELS: OnceLock<Vec<V3>> = OnceLock::new();
    TEXELS.get_or_init(|| {
        let sheet = image::load_from_memory(SHEET)
            .expect("the block sheet is a PNG")
            .to_rgba8();
        assert_eq!(sheet.dimensions(), (64, 64), "the block sheet is 4×4 tiles");
        sheet
            .pixels()
            .map(|p| {
                V3(f32::from(p[0]), f32::from(p[1]), f32::from(p[2])).map(|v| (v / 255.).powf(2.2))
            })
            .collect()
    })
}

/// The texel of block `id` under the point (`u`, `v`) of its face.
fn block(id: usize, u: f32, v: f32) -> V3 {
    let within = |f: f32| ((f * 16.) as usize).min(15);
    let (x, y) = (id % 4 * 16 + within(u), id / 4 * 16 + within(v));
    sheet()[y * 64 + x]
}

/// The last touches every scene gets: darker corners, screen gamma, and
/// brightness cut to a few levels with a checker between them.
fn finish(color: V3, x: u32, y: u32, width: u32, height: u32) -> V3 {
    let (qx, qy) = (
        (x as f32 + 0.5) / width as f32,
        (y as f32 + 0.5) / height as f32,
    );
    let corner = qx * (1. - qx) * 4. * qy * (1. - qy) * 4.;
    let color =
        (color * (0.86 + 0.14 * smoothstep(0., 0.35, corner))).map(|v| v.max(0.).powf(0.4545));

    let (ax, ay) = (x as f32, y as f32);
    let (bx, by) = ((ax * 0.5).floor(), (ay * 0.5).floor());
    let dither = (bx / 2. + by * by * 0.75).fract() * 0.25 + (ax / 2. + ay * ay * 0.75).fract();
    let light = color.max().max(1e-4);
    let color = color * ((light * 8. + dither * 0.9).floor() / 8. / light);
    color.map(|v| ((v * 12. + 0.5).floor() / 12.).clamp(0., 1.))
}

/// What the shaft looks like right now.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shaft {
    scene: Scene,
    seed: u64,
    accent: V3,
    time: f32,
    /// Blocks travelled so far.
    travel: f64,
    /// Where the pointer is over the picture, 0 to 1 each way.
    pointer: (f32, f32),
    /// 0 at rest, 1 once the game runs.
    launch: f32,
    /// 0 when the server is down, 1 when it is up.
    online: f32,
}

impl Shaft {
    /// The block at one spot of a wall (0 and 2 the sides, 1 the ceiling,
    /// 3 the floor), where no lamp or ore sits.
    fn plain(&self, wall: i64, column: i64, row: i64, grain: f32) -> usize {
        match self.scene {
            Scene::Mine => match wall {
                3 if grain > 0.45 => tile::GRAVEL,
                3 => tile::COBBLESTONE,
                1 => tile::DEEPSLATE,
                _ if grain > 0.74 => tile::ANDESITE,
                _ if grain < 0.2 => tile::COBBLESTONE,
                _ => tile::STONE,
            },
            Scene::Deep => match wall {
                3 if grain > 0.6 => tile::GRAVEL,
                1 | 3 => tile::DEEPSLATE,
                _ if grain > 0.86 => tile::COBBLESTONE,
                _ => tile::DEEPSLATE,
            },
            // Trunks every few blocks, leaves between and overhead.
            Scene::Forest => match wall {
                3 if grain > 0.82 => tile::DIRT,
                3 => tile::GRASS,
                1 => tile::LEAVES,
                _ if column.rem_euclid(4) == 1 && row < 5 => tile::LOG,
                _ if row == 0 && grain > 0.5 => tile::DIRT,
                _ => tile::LEAVES,
            },
            Scene::Desert => match wall {
                3 if grain > 0.85 => tile::GRAVEL,
                3 => tile::SAND,
                1 if grain > 0.5 => tile::ANDESITE,
                1 => tile::STONE,
                _ if row >= 4 && grain > 0.4 => tile::STONE,
                _ if grain > 0.9 => tile::GRAVEL,
                _ => tile::SAND,
            },
            Scene::Snow => match wall {
                3 => tile::SNOW,
                1 if grain > 0.3 => tile::SNOW,
                1 => tile::STONE,
                _ if row == 0 || grain > 0.7 => tile::SNOW,
                _ if grain < 0.15 => tile::ANDESITE,
                _ => tile::STONE,
            },
        }
    }

    /// What lies beyond the far end before the server colors it.
    fn distance(&self) -> V3 {
        match self.scene {
            Scene::Mine => V3(0.015, 0.02, 0.035),
            Scene::Deep => V3(0.006, 0.008, 0.016),
            Scene::Forest => V3(0.02, 0.05, 0.03),
            Scene::Desert => V3(0.07, 0.045, 0.02),
            Scene::Snow => V3(0.06, 0.08, 0.11),
        }
    }

    fn pixel(&self, x: u32, y: u32, width: u32, height: u32) -> V3 {
        let (w, h) = (width as f32, height as f32);
        let (launch, online) = (self.launch, self.online);
        // Up is positive, as the walls below expect.
        let mut ux = (x as f32 + 0.5 - 0.5 * w) / h;
        let mut uy = (0.5 * h - y as f32 - 0.5) / h;
        ux -= (self.pointer.0 - 0.5) * 0.22;
        uy -= (0.5 - self.pointer.1) * 0.16 + 0.07;
        // The cart sways a little as it rolls.
        let (sin, cos) = ((self.travel * 0.05).sin() as f32 * 0.04).sin_cos();
        let (ux, uy) = (cos * ux + sin * uy, cos * uy - sin * ux);

        let (ax, ay) = (ux.abs(), uy.abs() * 1.25);
        let reach = ax.max(ay).max(1e-4);
        let depth = 0.4 / reach;
        let side = ay <= ax;
        let across = if side { uy * 1.25 / reach } else { ux / reach };
        // 0 and 2 are the side walls, 1 the ceiling, 3 the floor.
        let wall: i64 = match (side, ux > 0., uy > 0.) {
            (true, true, _) => 0,
            (true, false, _) => 2,
            (false, _, true) => 1,
            (false, _, false) => 3,
        };

        let along = f64::from(depth) * 3. + self.travel;
        let up = (across * 0.5 + 0.5) * 6.;
        let (column, row) = (along.floor() as i64, up.floor() as i64);
        let (fu, fv) = ((along - along.floor()) as f32, up - up.floor());
        let spot = row * 4 + wall;
        let grain = chance(self.seed, column, spot, 0);

        let lamp = side && column.rem_euclid(8) == 0 && row == 3;
        // Ore: more blocks of it as the launch nears.
        // Only stone holds any, and the deep holds more.
        let rich = match self.scene {
            Scene::Mine | Scene::Snow => 0.,
            Scene::Deep => 0.04,
            Scene::Desert => -0.04,
            Scene::Forest => -1.,
        };
        let ore = !lamp && chance(self.seed, column, spot, 1) >= 0.9 - rich - launch * 0.35;
        let diamonds = match self.scene {
            Scene::Deep | Scene::Snow => 0.45,
            Scene::Desert => 1.,
            _ => 0.8,
        };
        let id = if lamp {
            tile::GLOWSTONE
        } else if ore {
            if grain > diamonds {
                tile::DIAMOND_ORE
            } else {
                tile::GOLD_ORE
            }
        } else {
            self.plain(wall, column, row, grain)
        };
        let texel = block(id, fu, fv);

        let mut color = texel * 0.4;
        // A lamp every eighth block warms the stone around it.
        let from_lamp = ((along / 8.).fract() as f32 - 0.5) * 8.;
        let warmth = (-from_lamp * from_lamp * 0.9).exp() * online;
        color = color * V3(1. + 1.7 * warmth, 1. + 1.1 * warmth, 1. + 0.5 * warmth);
        // The walls beside the camera fall into shadow.
        color = color * (0.3 + 0.7 * smoothstep(0.4, 1.6, depth));
        if ore && texel.max() - texel.min() >= 0.12 {
            let pulse = 0.7 + 0.3 * (self.time * 2. + grain * std::f32::consts::TAU).sin();
            color = color + texel * (pulse * (0.3 + launch * 0.6));
        }
        if lamp {
            color = texel * (0.2 + 0.9 * online);
        }

        let far = self.distance().mix(
            self.accent * 0.8,
            (online * 0.12 + launch * 0.7).clamp(0., 1.),
        );
        color = far.mix(color, (-depth * 0.11).exp());
        // The far end glows: cold while the server sleeps, warm when it is up.
        let end =
            (V3(0.5, 0.6, 0.9) * 0.25).mix(self.accent, (online * 0.5 + launch).clamp(0., 1.));
        color = color + end * (smoothstep(0.09, 0., reach) * (0.35 + launch));
        // Streaks when the launch speeds things up.
        if chance(self.seed, (across * 40.).floor() as i64, wall, 2) >= 0.97 {
            color = color + self.accent * (launch * 0.25 * smoothstep(0.1, 0.5, reach));
        }
        finish(color * 2.4, x, height - 1 - y, width, height)
    }
}

/// The skin nobody has uploaded yet: a plain figure in the launcher's
/// colors.
fn blank_skin(accent: V3) -> RgbaImage {
    let mut skin = RgbaImage::new(64, 64);
    let mut fill = |x: u32, y: u32, w: u32, h: u32, color: V3| {
        let [b, g, r, a] = color.bgra();
        for py in y..y + h {
            for px in x..x + w {
                skin.put_pixel(px, py, image::Rgba([r, g, b, a]));
            }
        }
    };
    let (bone, slate, ink) = (
        V3(0.91, 0.88, 0.81),
        V3(0.3, 0.31, 0.36),
        V3(0.1, 0.1, 0.12),
    );
    fill(0, 0, 32, 16, bone);
    fill(10, 12, 1, 1, ink);
    fill(13, 12, 1, 1, ink);
    fill(16, 16, 24, 16, accent);
    fill(40, 16, 16, 16, bone);
    fill(32, 48, 16, 16, bone);
    fill(0, 16, 16, 16, slate);
    fill(16, 48, 16, 16, slate);
    skin
}

/// Brings any skin to the 64×64 layout: the old half-height sheets have one
/// arm and one leg, worn on both sides.
pub fn whole_skin(skin: RgbaImage) -> Option<RgbaImage> {
    match skin.dimensions() {
        (64, 64) => Some(skin),
        (64, 32) => {
            let mut whole = RgbaImage::new(64, 64);
            image::imageops::replace(&mut whole, &skin, 0, 0);
            let limb = |x: u32| image::imageops::crop_imm(&skin, x, 16, 16, 16).to_image();
            image::imageops::replace(&mut whole, &limb(0), 16, 48);
            image::imageops::replace(&mut whole, &limb(40), 32, 48);
            Some(whole)
        }
        _ => None,
    }
}

/// The player's skin, standing on a checkered floor.
struct Figure<'a> {
    skin: &'a RgbaImage,
    slim: bool,
    /// Where the camera stands, and its three axes.
    from: [f32; 3],
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / len, v[1] / len, v[2] / len]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// One ray against the figure: the nearest opaque texel it meets.
struct Ray<'a> {
    skin: &'a RgbaImage,
    from: [f32; 3],
    dir: [f32; 3],
    nearest: f32,
    hit: Option<V3>,
}

impl Ray<'_> {
    /// A body part: a box at `center` of `size`, textured from `origin` on
    /// the skin. `puff` inflates it, which is how the outer layer is worn.
    fn part(&mut self, center: [f32; 3], size: [f32; 3], origin: (f32, f32), puff: f32) {
        let mut near = [0f32; 3];
        let mut far = [0f32; 3];
        for axis in 0..3 {
            let half = size[axis] * 0.5 + puff;
            let p = self.from[axis] - center[axis];
            let (t0, t1) = ((-half - p) / self.dir[axis], (half - p) / self.dir[axis]);
            near[axis] = t0.min(t1);
            far[axis] = t0.max(t1);
        }
        let enter = near[0].max(near[1]).max(near[2]);
        let leave = far[0].min(far[1]).min(far[2]);
        if enter > leave || enter < 0. || enter > self.nearest {
            return;
        }
        // Where it entered, in texels from the part's low corner.
        let mut f = [0f32; 3];
        for axis in 0..3 {
            let half = size[axis] * 0.5 + puff;
            let at = self.from[axis] - center[axis] + self.dir[axis] * enter + half;
            f[axis] = (at * size[axis] / (size[axis] + 2. * puff)).clamp(0.01, size[axis] - 0.01);
        }
        let [sx, sy, sz] = size;
        let [dx, dy, dz] = self.dir;
        let (u, v, shade) = if enter == near[1] {
            if dy < 0. {
                (sz + f[0], f[2], 1.)
            } else {
                (sz + sx + f[0], f[2], 0.45)
            }
        } else if enter == near[2] {
            if dz < 0. {
                (sz + f[0], sz + sy - f[1], 0.86)
            } else {
                (2. * sz + 2. * sx - f[0], sz + sy - f[1], 0.86)
            }
        } else if dx < 0. {
            (2. * sz + sx - f[2], sz + sy - f[1], 0.66)
        } else {
            (f[2], sz + sy - f[1], 0.66)
        };
        let (x, y) = ((origin.0 + u) as u32, (origin.1 + v) as u32);
        let Some(texel) = self.skin.get_pixel_checked(x, y) else {
            return;
        };
        if texel[3] < 128 {
            return;
        }
        self.nearest = enter;
        self.hit = Some(
            V3(
                f32::from(texel[0]),
                f32::from(texel[1]),
                f32::from(texel[2]),
            ) * (shade / 255.),
        );
    }
}

impl<'a> Figure<'a> {
    /// The figure as seen with the pointer at `pointer` (0 to 1 each way):
    /// moving it across walks the camera around.
    fn new(skin: &'a RgbaImage, slim: bool, pointer: (f32, f32)) -> Self {
        let yaw = 0.5 + (pointer.0 - 0.5) * 6.;
        let pitch = 0.14 + (0.5 - pointer.1) * 0.5;
        let target = [0., 15., 0.];
        let from = [
            target[0] + 72. * yaw.sin() * pitch.cos(),
            target[1] + 72. * pitch.sin(),
            target[2] + 72. * yaw.cos() * pitch.cos(),
        ];
        let forward = norm([
            target[0] - from[0],
            target[1] - from[1],
            target[2] - from[2],
        ]);
        let right = norm(cross(forward, [0., 1., 0.]));
        let up = cross(right, forward);
        Self {
            skin,
            slim,
            from,
            forward,
            right,
            up,
        }
    }

    fn pixel(&self, x: u32, y: u32, width: u32, height: u32) -> V3 {
        let (w, h) = (width as f32, height as f32);
        let ux = (x as f32 + 0.5 - 0.5 * w) / h;
        let uy = (0.5 * h - y as f32 - 0.5) / h;
        let (from, forward, right, up) = (self.from, self.forward, self.right, self.up);
        let dir = norm([
            forward[0] * 1.7 + ux * right[0] + uy * up[0],
            forward[1] * 1.7 + ux * right[1] + uy * up[1],
            forward[2] * 1.7 + ux * right[2] + uy * up[2],
        ])
        // Never exactly along an axis, so no division by zero below.
        .map(|v| v + 1e-5);

        let mut ray = Ray {
            skin: self.skin,
            from,
            dir,
            nearest: 1e5,
            hit: None,
        };
        let arm = if self.slim { 3. } else { 4. };
        let reach = 4. + arm * 0.5;
        // Each part twice: the skin itself, then the layer worn over it.
        ray.part([0., 28., 0.], [8., 8., 8.], (0., 0.), 0.);
        ray.part([0., 28., 0.], [8., 8., 8.], (32., 0.), 0.5);
        ray.part([0., 18., 0.], [8., 12., 4.], (16., 16.), 0.);
        ray.part([0., 18., 0.], [8., 12., 4.], (16., 32.), 0.25);
        ray.part([-reach, 18., 0.], [arm, 12., 4.], (40., 16.), 0.);
        ray.part([-reach, 18., 0.], [arm, 12., 4.], (40., 32.), 0.25);
        ray.part([reach, 18., 0.], [arm, 12., 4.], (32., 48.), 0.);
        ray.part([reach, 18., 0.], [arm, 12., 4.], (48., 48.), 0.25);
        ray.part([-2., 6., 0.], [4., 12., 4.], (0., 16.), 0.);
        ray.part([-2., 6., 0.], [4., 12., 4.], (0., 32.), 0.25);
        ray.part([2., 6., 0.], [4., 12., 4.], (16., 48.), 0.);
        ray.part([2., 6., 0.], [4., 12., 4.], (0., 48.), 0.25);
        // The skin keeps its own colors: no dithering over it.
        if let Some(hit) = ray.hit {
            return hit;
        }

        let rise = 1. - (y as f32 + 0.5) / h;
        let mut color = V3(0.012, 0.013, 0.018).mix(V3(0.06, 0.064, 0.08), rise);
        if dir[1] < 0. {
            let t = -from[1] / dir[1];
            let (gx, gz) = (from[0] + dir[0] * t, from[2] + dir[2] * t);
            if gx.abs().max(gz.abs()) < 16. {
                let odd = ((gx / 4.).floor() + (gz / 4.).floor()).rem_euclid(2.);
                color = V3(0.10, 0.105, 0.13).mix(V3(0.16, 0.168, 0.2), odd);
                if gx.abs() < 7. && gz.abs() < 3. {
                    color = color * 0.35;
                }
            }
        }
        finish(color, x, height - 1 - y, width, height)
    }
}

/// Draws `width`×`height` blocks and blows each up to `block` screen pixels.
/// The rows are shared out between the processor's cores.
fn frame(
    width: u32,
    height: u32,
    block: u32,
    pixel: impl Fn(u32, u32) -> V3 + Sync,
) -> Option<RgbaImage> {
    let (out_width, out_height) = (width * block, height * block);
    // One row of blocks, as bytes on the screen.
    let band = out_width as usize * block as usize * 4;
    let mut bytes = vec![0u8; band * height as usize];
    if bytes.is_empty() {
        return None;
    }
    let draw = |y: u32, rows: &mut [u8]| {
        let line = out_width as usize * 4;
        let (first, rest) = rows.split_at_mut(line);
        for x in 0..width {
            let texel = pixel(x, y).bgra();
            let at = (x * block) as usize * 4;
            for copy in first[at..at + block as usize * 4].as_chunks_mut::<4>().0 {
                *copy = texel;
            }
        }
        for copy in rest.chunks_exact_mut(line) {
            copy.copy_from_slice(first);
        }
    };

    let workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .clamp(1, 8)
        .min(height as usize);
    let share = (height as usize).div_ceil(workers);
    std::thread::scope(|scope| {
        let draw = &draw;
        let mut shares = bytes.chunks_mut(share * band).enumerate();
        // The first share is drawn here, while the others are drawn elsewhere.
        let mine = shares.next();
        for (ix, rows) in shares {
            scope.spawn(move || {
                for (y, rows) in rows.chunks_exact_mut(band).enumerate() {
                    draw((ix * share + y) as u32, rows);
                }
            });
        }
        if let Some((_, rows)) = mine {
            for (y, rows) in rows.chunks_exact_mut(band).enumerate() {
                draw(y as u32, rows);
            }
        }
    });
    RgbaImage::from_raw(out_width, out_height, bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Shaft,
    Figure,
}

/// What moves between frames, and what the last frame left behind.
#[derive(Default)]
struct Motion {
    last: Option<Instant>,
    time: f32,
    travel: f64,
    launch: f32,
    online: f32,
    pointer: (f32, f32),
    aim: (f32, f32),
    bounds: Option<Bounds<Pixels>>,
    shown: Option<Arc<RenderImage>>,
}

/// Everything a frame is drawn from, copied out for the painter.
#[derive(Clone)]
struct Shot {
    subject: Subject,
    scene: Scene,
    seed: u64,
    accent: V3,
    launch: f32,
    online: f32,
    skin: Option<Arc<RgbaImage>>,
    slim: bool,
}

/// One of the pictures, kept moving.
pub struct Stage {
    shot: Shot,
    motion: Rc<RefCell<Motion>>,
}

impl Stage {
    pub fn new(subject: Subject) -> Self {
        let motion = Rc::new(RefCell::new(Motion {
            aim: (0.5, 0.5),
            ..Motion::default()
        }));
        Self {
            shot: Shot {
                subject,
                scene: Scene::default(),
                seed: 0,
                accent: V3(0.949, 0.706, 0.255),
                launch: 0.,
                online: 0.,
                skin: None,
                slim: false,
            },
            motion,
        }
    }

    pub fn set_accent(&mut self, accent: Hsla) {
        let rgb = Rgba::from(accent);
        self.shot.accent = V3(rgb.r, rgb.g, rgb.b);
    }

    /// Which shaft and what it is dug through, how far into a launch
    /// (0 to 1), and whether the server is up.
    pub fn set_shaft(&mut self, scene: Scene, seed: u64, launch: f32, online: bool) {
        self.shot.scene = scene;
        self.shot.seed = seed;
        self.shot.launch = launch.clamp(0., 1.);
        self.shot.online = if online { 1. } else { 0. };
    }

    pub fn set_figure(&mut self, skin: Option<Arc<RgbaImage>>, slim: bool) {
        self.shot.skin = skin;
        self.shot.slim = slim;
    }
}

fn paint(shot: &Shot, motion: &RefCell<Motion>, bounds: Bounds<Pixels>, window: &mut Window) {
    let scale = window.scale_factor();
    let coarse = match shot.subject {
        Subject::Shaft => 4.,
        Subject::Figure => 3.,
    };
    let block = ((coarse * scale).round() as u32).max(1);
    let device = |length: Pixels| (f32::from(length) * scale).ceil() as u32;
    let (width, height) = (
        device(bounds.size.width).div_ceil(block),
        device(bounds.size.height).div_ceil(block),
    );
    if width == 0 || height == 0 {
        return;
    }

    let mut motion = motion.borrow_mut();
    let now = Instant::now();
    match motion.last.replace(now) {
        Some(last) => {
            let dt = now.duration_since(last).as_secs_f32().min(0.1);
            let ease = |rate: f32| (dt * rate).min(1.);
            motion.time += dt;
            motion.online += (shot.online - motion.online) * ease(2.5);
            motion.launch += (shot.launch - motion.launch) * ease(3.);
            motion.pointer.0 += (motion.aim.0 - motion.pointer.0) * ease(5.);
            motion.pointer.1 += (motion.aim.1 - motion.pointer.1) * ease(5.);
            motion.travel += f64::from(dt * (1.2 + motion.launch * 9.));
        }
        // The first frame shows things as they are, not on their way there.
        None => {
            motion.online = shot.online;
            motion.launch = shot.launch;
            motion.pointer = motion.aim;
        }
    }
    motion.bounds = Some(bounds);
    // Keep moving for as long as the picture is on screen: one frame of
    // the scene for every frame of the display.
    window.request_animation_frame();

    let picture = match shot.subject {
        Subject::Shaft => {
            let shaft = Shaft {
                scene: shot.scene,
                seed: shot.seed,
                accent: shot.accent,
                time: motion.time,
                travel: motion.travel,
                pointer: motion.pointer,
                launch: motion.launch,
                online: motion.online,
            };
            frame(width, height, block, |x, y| {
                shaft.pixel(x, y, width, height)
            })
        }
        Subject::Figure => {
            let blank;
            let skin = match &shot.skin {
                Some(skin) => skin.as_ref(),
                None => {
                    blank = blank_skin(shot.accent);
                    &blank
                }
            };
            let figure = Figure::new(skin, shot.slim, motion.pointer);
            frame(width, height, block, |x, y| {
                figure.pixel(x, y, width, height)
            })
        }
    };
    let Some(picture) = picture else { return };
    let picture = Arc::new(RenderImage::new(vec![Frame::new(picture)]));

    // Whole blocks only: the last row and column run past the edge and are
    // cut there, so every block is the same size.
    let drawn = Bounds::new(
        bounds.origin,
        size(
            px((width * block) as f32 / scale),
            px((height * block) as f32 / scale),
        ),
    );
    if let Err(err) =
        window.paint_image(bounds, drawn, Corners::default(), picture.clone(), 0, false)
    {
        tracing::warn!("could not draw the scene: {err}");
    }
    // Each frame is a new picture; the one before it is done with.
    if let Some(old) = motion.shown.replace(picture) {
        window.drop_image(old).ok();
    }
}

impl Render for Stage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (shot, motion) = (self.shot.clone(), self.motion.clone());
        div()
            .size_full()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, _| {
                let mut motion = this.motion.borrow_mut();
                let Some(bounds) = motion.bounds else { return };
                let within = |at: Pixels, from: Pixels, length: Pixels| {
                    ((at - from) / length.max(px(1.))).clamp(0., 1.)
                };
                motion.aim = (
                    within(event.position.x, bounds.origin.x, bounds.size.width),
                    within(event.position.y, bounds.origin.y, bounds.size.height),
                );
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _| paint(&shot, &motion, bounds, window),
                )
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaft(seed: u64) -> Shaft {
        Shaft {
            scene: Scene::default(),
            seed,
            accent: V3(0.949, 0.706, 0.255),
            time: 0.,
            travel: 0.,
            pointer: (0.5, 0.5),
            launch: 0.,
            online: 1.,
        }
    }

    fn picture(shaft: &Shaft) -> Vec<V3> {
        let (w, h) = (48, 32);
        (0..w * h)
            .map(|i| shaft.pixel(i % w, i / w, w, h))
            .collect()
    }

    #[test]
    fn a_server_keeps_its_shaft() {
        let (a, b) = (seed_of("survival"), seed_of("creative"));
        assert_ne!(a, b);
        assert_eq!(picture(&shaft(a)), picture(&shaft(a)));
        assert_ne!(picture(&shaft(a)), picture(&shaft(b)));
    }

    #[test]
    fn every_pixel_is_a_color() {
        let mut moved = shaft(7);
        moved.travel = 123_456.7;
        moved.launch = 1.;
        for color in picture(&moved) {
            for channel in [color.0, color.1, color.2] {
                assert!((0. ..=1.).contains(&channel), "{channel}");
            }
        }
    }

    #[test]
    fn chances_are_spread_out() {
        let draws: Vec<f32> = (0..1000).map(|i| chance(1, i, 3, 0)).collect();
        assert!(draws.iter().all(|v| (0. ..1.).contains(v)));
        let mean = draws.iter().sum::<f32>() / 1000.;
        assert!((0.45..0.55).contains(&mean), "{mean}");
    }

    #[test]
    fn frames_are_made_of_whole_blocks() {
        let image = frame(3, 2, 4, |x, y| V3(x as f32 / 2., y as f32, 0.)).unwrap();
        assert_eq!(image.dimensions(), (12, 8));
        assert_eq!(image.get_pixel(0, 0), image.get_pixel(3, 3));
        assert_ne!(image.get_pixel(3, 0), image.get_pixel(4, 0));
        // Blue comes first in what the window is given.
        assert_eq!(image.get_pixel(11, 7).0, [0, 255, 255, 255]);
    }

    #[test]
    fn the_figure_stands_in_the_middle() {
        let skin = blank_skin(V3(0.949, 0.706, 0.255));
        let figure = Figure::new(&skin, false, (0.5, 0.5));
        let (w, h) = (60, 80);
        // The chest is the accent color, shaded but not dithered.
        let chest = figure.pixel(w / 2, h / 2, w, h);
        assert!(chest.0 > chest.2 * 2., "{chest:?}");
        // The corner is the dark room behind.
        assert!(figure.pixel(0, 0, w, h).max() < 0.5);
    }

    #[test]
    fn old_skins_grow_a_second_side() {
        let mut old = RgbaImage::new(64, 32);
        old.put_pixel(44, 20, image::Rgba([9, 9, 9, 255]));
        let whole = whole_skin(old).unwrap();
        assert_eq!(whole.dimensions(), (64, 64));
        assert_eq!(whole.get_pixel(36, 52).0, [9, 9, 9, 255]);
        assert!(whole_skin(RgbaImage::new(10, 10)).is_none());
    }
}
