use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::{PI, TAU};

pub const CELL: u32 = 64;
pub const COLUMNS: u32 = 16;
const MIPS: u32 = 4;

macro_rules! icons {
    ($($name:ident),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon { $($name),* }
        impl Icon {
            pub const ALL: &'static [Icon] = &[$(Icon::$name),*];
            pub fn index(self) -> usize { self as usize }
        }
    };
}

icons! {
    Food, Logs, Sticks, Mud, Baskets, Rope, Tools,
    CatBasics, CatWater, CatForest, CatBuilding, CatCraft, CatTransport, CatDefence,
    Dam, Lodge, Burrow, FoodCache, ScentMound, GuardPost, Canal, Lock, WaterWheel, Workshop,
    Warehouse, Dock, Barge, LoggingCamp, ForagerHut, MudPit, Mill, Storehouse,
    ForesterLodge, TreeNursery,
    RoleForager, RoleLogger, RoleMudDigger, RoleBuilder, RoleHauler, RoleCrafter,
    RoleGuard, RoleForester,
    ViewWorld, ViewWater, ViewGoods, ViewVegetation, ViewForest, ViewWorkAreas, ViewHousing,
    ViewElevation,
    Idle, Unstaffed, Blocked, Full, NoStore, NoPower, Padlock, HandLock, Warn, Urgent, Up, Down,
    Dot, DotEmpty, Check, Cross,
    Pause, Play, Fast2, Fast3, Undo, Search, Pin, PriorityUp, PriorityDown, Plus, Minus, Focus,
    Clear, Expand, Collapse, Colony, Jobs, Journal, Log, Follow, Area, Haul, Balance, Menu, Back, Settings,
    Beaver, Adult, Yearling, Kit, MoodGood, MoodOk, MoodBad, Health,
    Spring, Summer, Autumn, Winter, DrySummer,
    Hourglass, Walk, Tree, Plant, MudBank, Hammer, Crest, Rise, Flood, Push, Drop, Site,
    Building, Home, Profile, Upstream, North, Gauge, Wheel,
    Sapling, TreeYoung, TreeOld, Coppice,
}

#[derive(Resource, Clone)]
pub struct Icons {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
}

pub struct IconsPlugin;

impl Plugin for IconsPlugin {
    fn build(&self, _app: &mut App) {}

    fn finish(&self, app: &mut App) {
        let world = app.world_mut();
        let image = match world.get_resource_mut::<Assets<Image>>() {
            Some(mut images) => images.add(atlas_image()),
            None => Handle::default(),
        };
        let layout = match world.get_resource_mut::<Assets<TextureAtlasLayout>>() {
            Some(mut layouts) => layouts.add(atlas_layout()),
            None => Handle::default(),
        };
        world.insert_resource(Icons { image, layout });
    }
}

pub fn icon_node(icons: &Icons, icon: Icon, px: f32, color: Color) -> (ImageNode, Node) {
    (
        ImageNode::from_atlas_image(icons.image.clone(), TextureAtlas { layout: icons.layout.clone(), index: icon.index() })
            .with_color(color),
        Node { width: Val::Px(px), height: Val::Px(px), flex_shrink: 0.0, ..default() },
    )
}

fn atlas_rows() -> u32 {
    (Icon::ALL.len() as u32).div_ceil(COLUMNS)
}

fn atlas_layout() -> TextureAtlasLayout {
    TextureAtlasLayout::from_grid(UVec2::splat(CELL), COLUMNS, atlas_rows(), None, None)
}

pub fn rasterize_atlas() -> (u32, u32, Vec<u8>) {
    let (w, h) = (COLUMNS * CELL, atlas_rows() * CELL);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for px in rgba.chunks_exact_mut(4) {
        px[..3].copy_from_slice(&[255, 255, 255]);
    }
    for &icon in Icon::ALL {
        let alpha = rasterize_icon(icon);
        let (cx, cy) = ((icon.index() as u32 % COLUMNS) * CELL, (icon.index() as u32 / COLUMNS) * CELL);
        for y in 0..CELL {
            for x in 0..CELL {
                rgba[(((cy + y) * w + cx + x) * 4 + 3) as usize] = alpha[(y * CELL + x) as usize];
            }
        }
    }
    (w, h, rgba)
}

fn atlas_image() -> Image {
    let (w, h, base) = rasterize_atlas();
    let mut data = base.clone();
    let (mut lw, mut lh, mut level) = (w, h, base);
    for _ in 1..MIPS {
        let (nw, nh) = (lw / 2, lh / 2);
        let mut next = vec![255u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let a = |dx: u32, dy: u32| level[(((2 * y + dy) * lw + 2 * x + dx) * 4 + 3) as usize] as u32;
                next[((y * nw + x) * 4 + 3) as usize] = ((a(0, 0) + a(1, 0) + a(0, 1) + a(1, 1) + 2) / 4) as u8;
            }
        }
        data.extend_from_slice(&next);
        (lw, lh, level) = (nw, nh, next);
    }
    let mut img = Image::new(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0; (w * h * 4) as usize],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = MIPS;
    img.data = Some(data);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    img
}

fn rasterize_icon(icon: Icon) -> Vec<u8> {
    let mut c = Canvas::new(Xf::ID);
    draw(icon, &mut c);
    c.d.iter().map(|&d| ((0.5 - d).clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect()
}

struct S(Box<dyn Fn(Vec2) -> f32>);

impl S {
    fn new(f: impl Fn(Vec2) -> f32 + 'static) -> S {
        S(Box::new(f))
    }
    fn at(&self, p: Vec2) -> f32 {
        (self.0)(p)
    }
    fn or(self, o: S) -> S {
        S::new(move |p| self.at(p).min(o.at(p)))
    }
    fn minus(self, o: S) -> S {
        S::new(move |p| self.at(p).max(-o.at(p)))
    }
    fn and(self, o: S) -> S {
        S::new(move |p| self.at(p).max(o.at(p)))
    }
    fn grow(self, k: f32) -> S {
        S::new(move |p| self.at(p) - k)
    }
    fn outline(self, w: f32) -> S {
        S::new(move |p| self.at(p).abs() - w * 0.5)
    }
    fn rot(self, cx: f32, cy: f32, deg: f32) -> S {
        let c = v(cx, cy);
        let r = Vec2::from_angle(-deg.to_radians());
        S::new(move |p| self.at(c + r.rotate(p - c)))
    }
    fn above(self, y: f32) -> S {
        S::new(move |p| self.at(p).max(p.y - y))
    }
}

fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

fn union(shapes: Vec<S>) -> S {
    S::new(move |p| shapes.iter().map(|s| s.at(p)).fold(f32::MAX, f32::min))
}

fn circle(cx: f32, cy: f32, r: f32) -> S {
    let c = v(cx, cy);
    S::new(move |p| (p - c).length() - r)
}

fn ring(cx: f32, cy: f32, r: f32, w: f32) -> S {
    circle(cx, cy, r).outline(w)
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> S {
    let (c, r) = (v(cx, cy), v(rx, ry));
    S::new(move |p| {
        let q = (p - c).abs();
        let k0 = (q / r).length();
        let k1 = (q / (r * r)).length();
        if k1 < 1e-6 { -rx.min(ry) } else { k0 * (k0 - 1.0) / k1 }
    })
}

fn rect(cx: f32, cy: f32, hw: f32, hh: f32, rad: f32) -> S {
    let (c, h) = (v(cx, cy), v(hw, hh));
    S::new(move |p| {
        let q = (p - c).abs() - h + Vec2::splat(rad);
        q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - rad
    })
}

fn rect_c(x0: f32, y0: f32, x1: f32, y1: f32, rad: f32) -> S {
    rect((x0 + x1) * 0.5, (y0 + y1) * 0.5, (x1 - x0) * 0.5, (y1 - y0) * 0.5, rad)
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let (pa, ba) = (p - a, b - a);
    let t = (pa.dot(ba) / ba.dot(ba).max(1e-6)).clamp(0.0, 1.0);
    (pa - ba * t).length()
}

fn seg(x0: f32, y0: f32, x1: f32, y1: f32, w: f32) -> S {
    let (a, b) = (v(x0, y0), v(x1, y1));
    S::new(move |p| seg_dist(p, a, b) - w * 0.5)
}

fn polyline_v(pts: Vec<Vec2>, w: f32) -> S {
    S::new(move |p| pts.windows(2).map(|s| seg_dist(p, s[0], s[1])).fold(f32::MAX, f32::min) - w * 0.5)
}

fn polyline(pts: &[(f32, f32)], w: f32) -> S {
    polyline_v(pts.iter().map(|&(x, y)| v(x, y)).collect(), w)
}

fn poly_v(pts: Vec<Vec2>) -> S {
    S::new(move |p| {
        let n = pts.len();
        let mut d = (p - pts[0]).length_squared();
        let mut s = 1.0;
        let mut j = n - 1;
        for i in 0..n {
            let (vi, vj) = (pts[i], pts[j]);
            let e = vj - vi;
            let w = p - vi;
            let b = w - e * (w.dot(e) / e.dot(e).max(1e-9)).clamp(0.0, 1.0);
            d = d.min(b.dot(b));
            let c0 = p.y >= vi.y;
            let c1 = p.y < vj.y;
            let c2 = e.x * w.y > e.y * w.x;
            if (c0 && c1 && c2) || (!c0 && !c1 && !c2) {
                s = -s;
            }
            j = i;
        }
        s * d.sqrt()
    })
}

fn poly(pts: &[(f32, f32)]) -> S {
    poly_v(pts.iter().map(|&(x, y)| v(x, y)).collect())
}

fn arc(cx: f32, cy: f32, r: f32, a0: f32, a1: f32, w: f32) -> S {
    let c = v(cx, cy);
    let span = a1 - a0;
    let e0 = c + Vec2::from_angle(a0.to_radians()) * r;
    let e1 = c + Vec2::from_angle(a1.to_radians()) * r;
    S::new(move |p| {
        let q = p - c;
        let a = q.y.atan2(q.x).to_degrees();
        let t = (a - a0).rem_euclid(360.0);
        let d = if t <= span { (q.length() - r).abs() } else { (p - e0).length().min((p - e1).length()) };
        d - w * 0.5
    })
}

fn qbez_pts(a: Vec2, c: Vec2, b: Vec2, n: usize) -> Vec<Vec2> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            a * (1.0 - t) * (1.0 - t) + c * 2.0 * t * (1.0 - t) + b * t * t
        })
        .collect()
}

fn qbez(a: (f32, f32), c: (f32, f32), b: (f32, f32), w: f32) -> S {
    polyline_v(qbez_pts(v(a.0, a.1), v(c.0, c.1), v(b.0, b.1), 20), w)
}

fn leaf(a: (f32, f32), b: (f32, f32), width: f32) -> S {
    let (a, b) = (v(a.0, a.1), v(b.0, b.1));
    let m = (a + b) * 0.5;
    let n = (b - a).perp().normalize() * width;
    let mut pts = qbez_pts(a, m + n, b, 16);
    pts.extend(qbez_pts(b, m - n, a, 16).into_iter().skip(1));
    poly_v(pts)
}

fn wave(x0: f32, x1: f32, y: f32, amp: f32, periods: f32, w: f32) -> S {
    let pts = (0..=40)
        .map(|i| {
            let t = i as f32 / 40.0;
            v(x0 + (x1 - x0) * t, y - amp * (t * periods * TAU).sin())
        })
        .collect();
    polyline_v(pts, w)
}

fn wave_fill(x0: f32, x1: f32, ytop: f32, ybot: f32, amp: f32, periods: f32) -> S {
    let k = periods * TAU / (x1 - x0);
    S::new(move |p| {
        let t = (p.x - x0) * k;
        let top = ytop - amp * t.sin();
        let slope = amp * k * t.cos();
        let dtop = (top - p.y) / (1.0 + slope * slope).sqrt();
        let dx = (x0 - p.x).max(p.x - x1);
        dtop.max(p.y - ybot).max(dx)
    })
}

fn radial(n: usize, cx: f32, cy: f32, f: impl Fn() -> S) -> S {
    union((0..n).map(|i| f().rot(cx, cy, i as f32 * 360.0 / n as f32)).collect())
}

#[derive(Clone, Copy)]
struct Xf {
    o: Vec2,
    k: f32,
    r: Vec2,
}

impl Xf {
    const ID: Xf = Xf { o: Vec2::ZERO, k: 1.0, r: Vec2::X };
    fn to_local(&self, p: Vec2) -> Vec2 {
        v(self.r.x, -self.r.y).rotate(p - self.o) / self.k
    }
    fn then(&self, child: Xf) -> Xf {
        Xf { o: self.o + self.r.rotate(child.o) * self.k, k: self.k * child.k, r: self.r.rotate(child.r) }
    }
}

struct Canvas {
    d: Vec<f32>,
    xf: Xf,
}

const W: f32 = 5.5;
const GAP: f32 = 3.0;

impl Canvas {
    fn new(xf: Xf) -> Self {
        Canvas { d: vec![1e3; (CELL * CELL) as usize], xf }
    }
    fn each(&mut self, s: &S, op: impl Fn(f32, f32) -> f32) {
        for y in 0..CELL {
            for x in 0..CELL {
                let p = v(x as f32 + 0.5, y as f32 + 0.5);
                let i = (y * CELL + x) as usize;
                let d = s.at(self.xf.to_local(p)) * self.xf.k;
                self.d[i] = op(self.d[i], d);
            }
        }
    }
    fn add(&mut self, s: S) {
        self.each(&s, f32::min);
    }
    fn cut(&mut self, s: S) {
        self.each(&s, |a, b| a.max(-b));
    }
    fn knock(&mut self, s: S) {
        let g = GAP * self.xf.k;
        self.each(&s, move |a, b| a.max(g - b).min(b));
    }
    fn part(&mut self, cx: f32, cy: f32, k: f32, deg: f32, gap: f32, f: impl FnOnce(&mut Canvas)) {
        let r = Vec2::from_angle(deg.to_radians());
        let child = Xf { o: v(cx, cy) - r.rotate(v(32.0, 32.0)) * k, k, r };
        let mut sub = Canvas::new(self.xf.then(child));
        f(&mut sub);
        let g = gap * self.xf.k;
        for (a, &b) in self.d.iter_mut().zip(&sub.d) {
            *a = if gap > 0.0 { a.max(g - b).min(b) } else { a.min(b) };
        }
    }
    fn cut_part(&mut self, cx: f32, cy: f32, k: f32, deg: f32, grow: f32, f: impl FnOnce(&mut Canvas)) {
        let r = Vec2::from_angle(deg.to_radians());
        let child = Xf { o: v(cx, cy) - r.rotate(v(32.0, 32.0)) * k, k, r };
        let mut sub = Canvas::new(self.xf.then(child));
        f(&mut sub);
        let g = grow * self.xf.k;
        for (a, &b) in self.d.iter_mut().zip(&sub.d) {
            *a = a.max(g - b);
        }
    }
}

fn head(c: &mut Canvas) {
    c.add(circle(14.5, 16.0, 7.5).or(circle(49.5, 16.0, 7.5)));
    c.add(ellipse(32.0, 32.0, 22.5, 19.5));
    c.cut(circle(22.5, 27.5, 3.8).or(circle(41.5, 27.5, 3.8)));
    c.cut(ellipse(32.0, 37.5, 7.0, 4.5));
    c.knock(rect_c(26.0, 45.0, 38.0, 58.0, 2.0));
    c.cut(seg(32.0, 48.0, 32.0, 60.0, 2.2));
}

fn axe(c: &mut Canvas) {
    c.add(seg(34.0, 10.0, 34.0, 58.0, 7.5));
    c.add(poly(&[(32.0, 10.0), (17.0, 5.0), (9.0, 20.0), (17.0, 35.0), (32.0, 29.0)]).grow(1.5));
    c.add(rect_c(32.0, 12.0, 42.0, 26.0, 2.0));
}

fn hammer(c: &mut Canvas) {
    c.add(seg(32.0, 22.0, 32.0, 58.0, 8.0));
    c.add(rect_c(13.0, 7.0, 51.0, 25.0, 3.0));
}

fn shovel(c: &mut Canvas) {
    c.add(seg(32.0, 8.0, 32.0, 34.0, 7.0));
    c.add(seg(23.0, 7.0, 41.0, 7.0, 6.5));
    c.add(poly(&[(20.0, 30.0), (44.0, 30.0), (44.0, 45.0), (32.0, 57.0), (20.0, 45.0)]).grow(2.0));
}



fn saw(c: &mut Canvas) {
    let mut pts = vec![v(8.0, 26.0), v(44.0, 18.0), v(44.0, 42.0)];
    for i in 0..5 {
        let x = 44.0 - i as f32 * 7.2;
        pts.push(v(x - 3.6, 46.0 + 0.0 * x));
        pts.push(v(x - 7.2, 42.0));
    }
    pts.push(v(8.0, 36.0));
    c.add(poly_v(pts).grow(0.8));
    c.add(rect_c(42.0, 14.0, 58.0, 46.0, 6.0));
    c.cut(rect_c(47.0, 21.0, 53.0, 37.0, 3.0));
}

fn shield(c: &mut Canvas) {
    let mut pts = vec![v(10.0, 12.0), v(32.0, 5.0), v(54.0, 12.0)];
    pts.extend(qbez_pts(v(54.0, 12.0), v(55.0, 46.0), v(32.0, 59.0), 14).into_iter().skip(1));
    pts.extend(qbez_pts(v(32.0, 59.0), v(9.0, 46.0), v(10.0, 12.0), 14).into_iter().skip(1));
    c.add(poly_v(pts).grow(0.5));
}

fn berries(c: &mut Canvas) {
    c.add(circle(21.0, 44.0, 12.0));
    c.knock(circle(43.0, 41.0, 12.0));
    c.cut(circle(16.5, 40.0, 3.0).or(circle(38.5, 37.0, 3.0)));
    c.add(qbez((21.0, 33.0), (24.0, 18.0), (35.0, 10.0), 4.5));
    c.add(qbez((43.0, 30.0), (41.0, 18.0), (35.0, 10.0), 4.5));
    c.add(leaf((35.0, 11.0), (57.0, 7.0), 11.0));
}

fn house(c: &mut Canvas) {
    c.add(poly(&[(32.0, 8.0), (57.0, 30.0), (51.0, 30.0), (51.0, 56.0), (13.0, 56.0), (13.0, 30.0), (7.0, 30.0)]).grow(1.5));
}

fn house_door(c: &mut Canvas) {
    house(c);
    c.cut(rect_c(26.0, 39.0, 38.0, 62.0, 5.0));
}

fn crate_box(c: &mut Canvas) {
    c.add(rect_c(10.0, 12.0, 54.0, 54.0, 4.0));
    c.cut(seg(10.0, 26.0, 54.0, 26.0, 3.5));
    c.cut(seg(10.0, 40.0, 54.0, 40.0, 3.5));
}

fn padlock(c: &mut Canvas) {
    c.add(rect_c(14.0, 27.0, 50.0, 56.0, 5.0));
    c.add(arc(32.0, 25.0, 11.0, 180.0, 360.0, 6.5));
    c.add(seg(21.0, 25.0, 21.0, 29.0, 6.5).or(seg(43.0, 25.0, 43.0, 29.0, 6.5)));
    c.cut(circle(32.0, 38.0, 4.5));
    c.cut(poly(&[(30.0, 39.0), (34.0, 39.0), (35.5, 48.0), (28.5, 48.0)]));
}

fn pine(c: &mut Canvas) {
    c.add(
        poly(&[
            (32.0, 6.0),
            (46.0, 21.0),
            (40.0, 21.0),
            (52.0, 35.0),
            (45.0, 35.0),
            (56.0, 48.0),
            (8.0, 48.0),
            (19.0, 35.0),
            (12.0, 35.0),
            (24.0, 21.0),
            (18.0, 21.0),
        ])
        .grow(1.2),
    );
    c.add(rect_c(27.0, 44.0, 37.0, 58.0, 1.5));
}

fn round_tree(c: &mut Canvas) {
    c.add(seg(32.0, 34.0, 32.0, 58.0, 8.0));
    c.knock(circle(32.0, 22.0, 15.0).or(circle(19.0, 31.0, 11.5)).or(circle(45.0, 31.0, 11.5)).or(rect_c(19.0, 28.0, 45.0, 42.5, 8.0)));
}

fn seedling(c: &mut Canvas) {
    c.add(qbez((32.0, 56.0), (31.0, 42.0), (33.0, 26.0), 5.0));
    c.add(leaf((32.0, 36.0), (8.0, 20.0), 9.0));
    c.add(leaf((33.0, 28.0), (56.0, 9.0), 10.0));
    c.add(seg(14.0, 57.0, 50.0, 57.0, 4.5));
}

fn mud_pile(c: &mut Canvas) {
    c.add(union(vec![circle(20.0, 44.0, 12.0), circle(44.0, 44.0, 12.0), circle(32.0, 32.0, 14.0), rect_c(8.0, 42.0, 56.0, 56.0, 5.0)]));
    c.cut(circle(25.0, 37.0, 3.2).or(circle(40.0, 46.0, 3.0)).or(circle(18.0, 49.0, 2.3)));
}

fn lodge_dome(c: &mut Canvas) {
    c.add(seg(21.0, 7.0, 39.0, 30.0, 5.0));
    c.add(seg(43.0, 7.0, 25.0, 30.0, 5.0));
    c.knock(ellipse(32.0, 55.0, 28.0, 31.0).above(55.0));
    c.cut(seg(6.0, 45.0, 30.0, 40.0, 3.0));
    c.cut(seg(34.0, 40.0, 58.0, 45.0, 3.0));
    c.cut(seg(14.0, 33.0, 31.0, 30.0, 3.0));
    c.cut(seg(33.0, 30.0, 50.0, 33.0, 3.0));
}

fn wheel(c: &mut Canvas) {
    c.add(ring(32.0, 32.0, 18.0, 5.0));
    c.add(radial(4, 32.0, 32.0, || seg(32.0, 14.0, 32.0, 50.0, 4.0)).rot(32.0, 32.0, 22.5));
    c.add(circle(32.0, 32.0, 5.5));
    c.add(radial(8, 32.0, 32.0, || rect(32.0, 7.5, 4.5, 6.0, 1.5)));
}

fn drop_shape() -> S {
    circle(32.0, 40.0, 16.0).or(poly(&[(32.0, 5.0), (46.1, 32.5), (17.9, 32.5)]).grow(0.8))
}

fn heart() -> S {
    circle(21.5, 24.0, 12.0)
        .or(circle(42.5, 24.0, 12.0))
        .or(poly(&[(10.5, 28.5), (32.0, 52.0), (53.5, 28.5), (32.0, 24.0)]).grow(1.5))
}

fn sun(c: &mut Canvas, r: f32, ray0: f32, ray1: f32) {
    c.add(circle(32.0, 32.0, r));
    c.add(radial(8, 32.0, 32.0, || seg(32.0, 32.0 - ray0, 32.0, 32.0 - ray1, 5.5)));
}

fn face(c: &mut Canvas) {
    c.add(circle(32.0, 32.0, 26.0));
    c.cut(circle(23.0, 25.0, 4.2).or(circle(41.0, 25.0, 4.2)));
}

fn paw(c: &mut Canvas) {
    c.add(ellipse(32.0, 41.0, 13.0, 11.0));
    c.add(ellipse(14.0, 27.0, 5.0, 6.5).rot(14.0, 27.0, -25.0));
    c.add(ellipse(25.0, 17.0, 5.0, 6.5).rot(25.0, 17.0, -8.0));
    c.add(ellipse(39.0, 17.0, 5.0, 6.5).rot(39.0, 17.0, 8.0));
    c.add(ellipse(50.0, 27.0, 5.0, 6.5).rot(50.0, 27.0, 25.0));
}

fn slash(c: &mut Canvas) {
    c.knock(seg(10.0, 10.0, 54.0, 54.0, W));
}

fn tri_rounded(pts: &[(f32, f32)]) -> S {
    poly(pts).grow(2.5)
}

fn role(c: &mut Canvas, deg: f32, tool: impl FnOnce(&mut Canvas)) {
    c.part(23.0, 24.0, 0.66, 0.0, 0.0, head);
    c.part(41.0, 41.0, 0.66, deg, 3.5, tool);
}

fn draw(icon: Icon, c: &mut Canvas) {
    use Icon::*;
    match icon {
        Food => berries(c),
        Logs => {
            for (x, y) in [(19.0, 45.0), (45.0, 45.0), (32.0, 22.0)] {
                c.add(circle(x, y, 12.5));
                c.cut(ring(x, y, 7.0, 3.0));
            }
        }
        Sticks => {
            c.add(seg(10.0, 44.0, 42.0, 12.0, 4.8));
            c.add(seg(14.0, 52.0, 52.0, 14.0, 4.8));
            c.add(seg(22.0, 54.0, 54.0, 22.0, 4.8));
            c.add(seg(44.0, 20.0, 50.0, 6.0, 4.0));
            c.knock(seg(24.0, 26.0, 40.0, 42.0, 6.5));
        }
        Mud => mud_pile(c),
        Baskets => {
            c.add(poly(&[(12.0, 30.0), (52.0, 30.0), (47.0, 55.0), (17.0, 55.0)]).grow(2.0));
            c.cut(seg(10.0, 39.0, 54.0, 39.0, 3.5).or(seg(10.0, 47.0, 54.0, 47.0, 3.5)));
            c.cut(seg(32.0, 31.0, 32.0, 58.0, 3.5));
            c.knock(rect_c(7.0, 24.0, 57.0, 31.0, 3.0));
            c.add(arc(32.0, 26.0, 15.0, 180.0, 360.0, 5.0));
        }
        Rope => {
            let pts: Vec<Vec2> = (0..=160)
                .map(|i| {
                    let t = i as f32 / 160.0 * 2.25 * TAU;
                    let r = 4.0 + t * 9.3 / TAU;
                    v(30.0, 31.0) + Vec2::from_angle(t + PI) * r
                })
                .collect();
            let end = *pts.last().unwrap();
            c.add(polyline_v(pts, 5.2));
            c.add(qbez((end.x, end.y), (52.0, 56.0), (60.0, 52.0), 5.2));
        }
        Tools => {
            c.part(32.0, 32.0, 0.86, -42.0, 0.0, hammer);
            c.part(32.0, 32.0, 0.86, 42.0, 3.0, |c| {
                c.add(seg(32.0, 24.0, 32.0, 58.0, 8.0));
                c.add(circle(32.0, 17.0, 13.0));
                c.cut(rect_c(27.5, -4.0, 36.5, 15.0, 1.5));
            });
        }
        CatBasics => lodge_dome(c),
        CatWater => {
            c.add(wave(8.0, 56.0, 24.0, 4.5, 1.5, 6.0));
            c.add(wave(8.0, 56.0, 40.0, 4.5, 1.5, 6.0));
        }
        CatForest => pine(c),
        CatBuilding => {
            c.add(poly(&[(32.0, 10.0), (55.0, 30.0), (50.0, 30.0), (50.0, 54.0), (14.0, 54.0), (14.0, 30.0), (9.0, 30.0)]).outline(W));
            c.add(rect_c(26.0, 38.0, 38.0, 54.0, 2.0));
        }
        CatCraft => {
            c.add(
                poly(&[
                    (5.0, 17.0),
                    (56.0, 16.0),
                    (56.0, 28.0),
                    (43.0, 31.0),
                    (40.0, 40.0),
                    (48.0, 45.0),
                    (49.0, 52.0),
                    (15.0, 52.0),
                    (16.0, 45.0),
                    (24.0, 40.0),
                    (22.0, 31.0),
                    (15.0, 28.0),
                ])
                .grow(1.5),
            );
        }
        CatTransport => {
            c.add(poly(&[(5.0, 40.0), (59.0, 40.0), (50.0, 54.0), (14.0, 54.0)]).grow(1.5));
            c.add(seg(32.0, 7.0, 32.0, 40.0, 4.5));
            c.knock(poly(&[(36.0, 9.0), (36.0, 34.0), (54.0, 34.0)]).grow(1.0));
            c.knock(poly(&[(28.0, 15.0), (28.0, 34.0), (14.0, 34.0)]).grow(1.0));
        }
        CatDefence => shield(c),
        Dam => {
            for y in [17.0, 29.0, 41.0, 53.0] {
                c.add(wave(5.0, 23.0, y, 2.8, 1.0, 5.0));
            }
            for y in [17.0, 34.0, 51.0] {
                c.add(circle(35.0, y, 8.8));
                c.cut(ring(35.0, y, 4.3, 2.6));
            }
            c.add(wave(47.0, 60.0, 53.0, 2.8, 1.0, 5.0));
        }
        Lodge => {
            c.part(32.0, 26.0, 0.8, 0.0, 0.0, lodge_dome);
            c.cut(rect_c(25.0, 34.0, 39.0, 52.0, 7.0));
            c.cut(seg(0.0, 49.0, 64.0, 49.0, 4.0));
            c.add(wave(6.0, 58.0, 55.0, 3.0, 2.0, 5.0));
        }
        Burrow => {
            c.add(ellipse(32.0, 48.0, 25.0, 24.0).above(48.0));
            c.add(rect_c(4.0, 44.0, 60.0, 52.0, 3.0));
            c.cut(rect_c(22.0, 30.0, 42.0, 62.0, 10.0));
            c.add(seg(24.0, 58.0, 40.0, 58.0, 4.5));
        }
        FoodCache => {
            c.part(32.0, 42.0, 0.74, 0.0, 0.0, crate_box);
            c.part(33.0, 17.0, 0.52, 0.0, 3.0, berries);
        }
        ScentMound => {
            c.add(ellipse(32.0, 58.0, 24.0, 15.0).above(57.0));
            for x in [18.0, 32.0, 46.0] {
                let pts = (0..=24)
                    .map(|i| {
                        let t = i as f32 / 24.0;
                        v(x + 3.5 * (t * 1.5 * TAU).sin(), 37.0 - t * 30.0)
                    })
                    .collect();
                c.add(polyline_v(pts, 4.5));
            }
        }
        GuardPost => {
            c.add(poly(&[(32.0, 4.0), (53.0, 18.0), (11.0, 18.0)]).grow(1.5));
            c.add(rect_c(16.0, 20.0, 48.0, 33.0, 2.0));
            c.cut(rect_c(26.0, 23.0, 38.0, 30.0, 1.5));
            c.add(seg(21.0, 33.0, 13.0, 58.0, 5.5).or(seg(43.0, 33.0, 51.0, 58.0, 5.5)));
            c.add(seg(18.0, 46.0, 46.0, 46.0, 4.5));
        }
        Canal => {
            c.add(seg(10.0, 10.0, 10.0, 34.0, W).or(seg(54.0, 10.0, 54.0, 34.0, W)));
            c.add(arc(32.0, 34.0, 22.0, 0.0, 180.0, W));
            c.add(wave(19.0, 45.0, 28.0, 3.0, 1.0, 4.5));
            c.add(wave(19.0, 45.0, 40.0, 3.0, 1.0, 4.5));
        }
        Lock => {
            c.add(rect_c(28.0, 12.0, 36.0, 58.0, 2.0));
            c.add(seg(22.0, 8.0, 42.0, 8.0, 5.0));
            c.add(wave(5.0, 24.0, 22.0, 3.0, 1.0, 5.0));
            c.add(wave(5.0, 24.0, 33.0, 3.0, 1.0, 5.0));
            c.add(wave(5.0, 24.0, 44.0, 3.0, 1.0, 5.0));
            c.add(wave(40.0, 59.0, 44.0, 3.0, 1.0, 5.0));
            c.add(seg(4.0, 56.0, 60.0, 56.0, 4.5));
        }
        WaterWheel => {
            c.part(32.0, 27.0, 0.8, 0.0, 0.0, wheel);
            c.knock(wave(4.0, 60.0, 54.0, 3.0, 2.0, W));
        }
        Workshop => {
            c.add(rect_c(5.0, 36.0, 59.0, 43.0, 2.0));
            c.add(seg(12.0, 42.0, 12.0, 58.0, W).or(seg(52.0, 42.0, 52.0, 58.0, W)));
            c.add(seg(12.0, 51.0, 52.0, 51.0, 4.0));
            c.part(30.0, 18.0, 0.52, -45.0, 2.5, hammer);
            c.part(50.0, 24.0, 0.42, 0.0, 2.5, |c| c.add(rect_c(14.0, 14.0, 50.0, 50.0, 3.0)));
        }
        Warehouse => {
            c.add(poly(&[(4.0, 26.0), (32.0, 8.0), (60.0, 26.0), (56.0, 26.0), (56.0, 58.0), (8.0, 58.0), (8.0, 26.0)]).grow(1.0));
            c.cut(rect_c(18.0, 32.0, 46.0, 62.0, 2.0));
            c.add(
                (seg(20.0, 34.0, 44.0, 58.0, 4.0).or(seg(44.0, 34.0, 20.0, 58.0, 4.0)))
                    .and(rect_c(18.0, 32.0, 46.0, 58.0, 0.0)),
            );
        }
        Dock => {
            c.add(rect_c(5.0, 22.0, 59.0, 30.0, 2.0));
            c.add(seg(14.0, 30.0, 14.0, 52.0, W).or(seg(32.0, 30.0, 32.0, 52.0, W)).or(seg(50.0, 30.0, 50.0, 52.0, W)));
            c.knock(wave(4.0, 60.0, 44.0, 2.5, 2.5, 4.5));
            c.add(wave(4.0, 60.0, 56.0, 2.5, 2.5, 4.5));
        }
        Barge => {
            c.add(poly(&[(4.0, 34.0), (60.0, 34.0), (53.0, 47.0), (11.0, 47.0)]).grow(1.5));
            c.knock(rect_c(12.0, 14.0, 29.0, 31.0, 2.0));
            c.knock(rect_c(33.0, 20.0, 50.0, 31.0, 2.0));
            c.add(wave(4.0, 60.0, 56.0, 2.5, 2.0, 4.5));
        }
        LoggingCamp => {
            c.add(poly(&[(17.0, 34.0), (47.0, 34.0), (49.0, 50.0), (58.0, 57.0), (6.0, 57.0), (15.0, 50.0)]).grow(1.0));
            c.knock(ellipse(32.0, 33.0, 16.0, 5.5));
            c.part(37.0, 22.0, 0.6, 35.0, 3.0, axe);
        }
        ForagerHut => {
            c.part(26.0, 37.0, 0.8, 0.0, 0.0, house_door);
            c.part(45.0, 21.0, 0.58, 0.0, 3.0, berries);
        }
        MudPit => {
            c.part(28.0, 38.0, 0.82, 0.0, 0.0, mud_pile);
            c.part(44.0, 22.0, 0.62, 25.0, 3.0, shovel);
        }

        Mill => {
            c.add(poly(&[(25.0, 30.0), (39.0, 30.0), (46.0, 58.0), (18.0, 58.0)]).grow(1.0));
            c.cut(rect_c(28.0, 46.0, 36.0, 62.0, 4.0));
            c.knock(radial(4, 32.0, 26.0, || rect_c(33.0, 3.0, 42.0, 23.0, 1.5)).rot(32.0, 26.0, 45.0));
            c.knock(circle(32.0, 26.0, 4.5));
        }
        Storehouse => {
            for (x0, y0) in [(7.0, 34.0), (35.0, 34.0), (21.0, 9.0)] {
                c.add(rect_c(x0, y0, x0 + 22.0, y0 + 22.0, 3.0));
                c.cut(seg(x0, y0 + 11.0, x0 + 22.0, y0 + 11.0, 3.0));
            }
        }
        ForesterLodge => {
            c.part(26.0, 37.0, 0.8, 0.0, 0.0, house_door);
            c.part(46.0, 21.0, 0.6, 0.0, 3.0, seedling);
        }
        TreeNursery => {
            c.add(poly(&[(8.0, 40.0), (56.0, 40.0), (50.0, 56.0), (14.0, 56.0)]).grow(1.5));
            c.cut(seg(10.0, 47.0, 54.0, 47.0, 3.0));
            for x in [18.0, 32.0, 46.0] {
                c.part(x, 22.0, 0.42, 0.0, 2.5, seedling);
            }
        }
        RoleForager => role(c, 0.0, berries),
        RoleLogger => role(c, 35.0, axe),
        RoleMudDigger => role(c, 30.0, shovel),
        RoleBuilder => role(c, -35.0, hammer),
        RoleHauler => role(c, 0.0, crate_box),
        RoleCrafter => role(c, -30.0, saw),
        RoleGuard => role(c, 0.0, shield),
        RoleForester => role(c, 0.0, seedling),
        ViewWorld => {
            c.add(poly(&[(6.0, 14.0), (21.0, 8.0), (21.0, 50.0), (6.0, 56.0)]).grow(1.0));
            c.add(poly(&[(26.0, 8.0), (38.0, 14.0), (38.0, 56.0), (26.0, 50.0)]).grow(1.0));
            c.add(poly(&[(43.0, 14.0), (58.0, 8.0), (58.0, 50.0), (43.0, 56.0)]).grow(1.0));
        }
        ViewWater => {
            c.add(drop_shape().grow(1.5));
            c.cut(wave(12.0, 52.0, 40.0, 3.0, 1.0, 4.0));
        }
        ViewGoods => {
            c.add(poly(&[(32.0, 7.0), (55.0, 19.0), (55.0, 45.0), (32.0, 57.0), (9.0, 45.0), (9.0, 19.0)]).grow(1.0));
            c.cut(polyline(&[(9.0, 19.0), (32.0, 31.0), (55.0, 19.0)], 3.5).or(seg(32.0, 31.0, 32.0, 60.0, 3.5)));
        }
        ViewVegetation => {
            c.add(seg(8.0, 56.0, 56.0, 56.0, 4.5));
            c.add(leaf((32.0, 56.0), (31.0, 8.0), 8.0));
            c.add(leaf((26.0, 56.0), (10.0, 20.0), 7.0));
            c.add(leaf((38.0, 56.0), (54.0, 20.0), 7.0));
            c.add(leaf((20.0, 56.0), (6.0, 40.0), 5.0));
            c.add(leaf((44.0, 56.0), (58.0, 40.0), 5.0));
        }
        ViewForest => {
            c.part(22.0, 36.0, 0.7, 0.0, 0.0, pine);
            c.part(42.0, 31.0, 0.8, 0.0, 3.0, pine);
        }
        ViewWorkAreas => {
            c.add(radial(8, 32.0, 32.0, || arc(32.0, 32.0, 25.0, -14.0, 14.0, W)));
            c.part(32.0, 32.0, 0.55, -40.0, 0.0, hammer);
        }
        ViewHousing => {
            c.part(21.0, 38.0, 0.62, 0.0, 0.0, house_door);
            c.part(42.0, 30.0, 0.7, 0.0, 3.0, house_door);
        }
        ViewElevation => {
            c.add(ellipse(33.0, 37.0, 26.0, 19.0).outline(W - 1.0));
            c.add(ellipse(30.0, 32.0, 16.0, 11.5).outline(W - 1.0));
            c.add(circle(28.0, 29.0, 5.5));
        }
        Idle => {
            c.add(circle(30.0, 33.0, 23.0).minus(circle(43.0, 24.0, 19.0)));
        }
        Unstaffed => {
            c.add(ellipse(32.0, 56.0, 20.0, 20.0).above(55.0));
            c.knock(circle(32.0, 20.0, 10.5));
            slash(c);
        }
        Blocked => {
            c.add(circle(32.0, 32.0, 26.0));
            c.cut(rect_c(15.0, 28.0, 49.0, 36.0, 2.0));
        }
        Full => {
            c.add(rect_c(13.0, 6.0, 51.0, 58.0, 6.0).outline(W));
            for y in [13.5, 27.0, 40.5] {
                c.add(rect_c(21.0, y, 43.0, y + 10.0, 2.0));
            }
        }
        NoStore => {
            c.part(32.0, 32.0, 0.82, 0.0, 0.0, crate_box);
            slash(c);
        }
        NoPower => {
            c.add(poly(&[(37.0, 4.0), (14.0, 36.0), (29.0, 36.0), (25.0, 60.0), (50.0, 26.0), (35.0, 26.0), (42.0, 4.0)]).grow(1.0));
            slash(c);
        }
        Padlock => padlock(c),
        HandLock => {
            c.part(25.0, 25.0, 0.72, 0.0, 0.0, paw);
            c.part(44.0, 42.0, 0.6, 0.0, 3.5, padlock);
        }
        Warn => c.add(poly(&[(32.0, 9.0), (55.0, 32.0), (32.0, 55.0), (9.0, 32.0)]).grow(3.0)),
        Urgent => c.add(rect(32.0, 32.0, 21.0, 21.0, 3.0)),
        Up => c.add(tri_rounded(&[(32.0, 12.0), (53.0, 49.0), (11.0, 49.0)])),
        Down => c.add(tri_rounded(&[(32.0, 52.0), (11.0, 15.0), (53.0, 15.0)])),
        Dot => c.add(circle(32.0, 32.0, 19.0)),
        DotEmpty => c.add(ring(32.0, 32.0, 17.0, 6.5)),
        Check => c.add(polyline(&[(11.0, 33.0), (25.0, 47.0), (53.0, 17.0)], 8.0)),
        Cross => c.add(seg(14.0, 14.0, 50.0, 50.0, 8.0).or(seg(50.0, 14.0, 14.0, 50.0, 8.0))),
        Pause => c.add(rect_c(14.0, 10.0, 27.0, 54.0, 3.0).or(rect_c(37.0, 10.0, 50.0, 54.0, 3.0))),
        Play => c.add(tri_rounded(&[(19.0, 12.0), (52.0, 32.0), (19.0, 52.0)])),
        Fast2 => {
            c.add(tri_rounded(&[(7.0, 15.0), (29.0, 32.0), (7.0, 49.0)]));
            c.knock(tri_rounded(&[(32.0, 15.0), (54.0, 32.0), (32.0, 49.0)]));
        }
        Fast3 => {
            c.add(tri_rounded(&[(5.0, 18.0), (21.0, 32.0), (5.0, 46.0)]));
            c.knock(tri_rounded(&[(23.0, 18.0), (39.0, 32.0), (23.0, 46.0)]));
            c.knock(tri_rounded(&[(41.0, 18.0), (57.0, 32.0), (41.0, 46.0)]));
        }
        Undo => {
            c.add(arc(35.0, 36.0, 18.0, 180.0, 400.0, W + 0.5));
            c.add(tri_rounded(&[(7.0, 31.0), (27.0, 31.0), (17.0, 44.0)]));
        }
        Search => {
            c.add(ring(27.0, 27.0, 15.0, W + 0.5));
            c.add(seg(39.0, 39.0, 54.0, 54.0, 8.0));
        }
        Pin => c.part(32.0, 32.0, 1.0, 35.0, 0.0, |c| {
            c.add(rect_c(20.0, 6.0, 44.0, 14.0, 3.0));
            c.add(poly(&[(25.0, 13.0), (39.0, 13.0), (42.0, 34.0), (22.0, 34.0)]));
            c.add(rect_c(14.0, 33.0, 50.0, 40.0, 3.0));
            c.add(seg(32.0, 40.0, 32.0, 59.0, 4.0));
        }),
        PriorityUp => {
            c.add(polyline(&[(13.0, 34.0), (32.0, 15.0), (51.0, 34.0)], 6.5));
            c.add(polyline(&[(13.0, 51.0), (32.0, 32.0), (51.0, 51.0)], 6.5));
        }
        PriorityDown => {
            c.add(polyline(&[(13.0, 30.0), (32.0, 49.0), (51.0, 30.0)], 6.5));
            c.add(polyline(&[(13.0, 13.0), (32.0, 32.0), (51.0, 13.0)], 6.5));
        }
        Plus => c.add(seg(32.0, 11.0, 32.0, 53.0, 7.5).or(seg(11.0, 32.0, 53.0, 32.0, 7.5))),
        Minus => c.add(seg(11.0, 32.0, 53.0, 32.0, 7.5)),
        Focus => {
            c.add(ring(32.0, 32.0, 17.0, W));
            c.add(seg(32.0, 5.0, 32.0, 18.0, W).or(seg(32.0, 46.0, 32.0, 59.0, W)));
            c.add(seg(5.0, 32.0, 18.0, 32.0, W).or(seg(46.0, 32.0, 59.0, 32.0, W)));
            c.add(circle(32.0, 32.0, 4.5));
        }
        Clear => {
            c.part(33.0, 28.0, 1.0, -40.0, 0.0, |c| {
                c.add(rect_c(8.0, 20.0, 56.0, 42.0, 5.0));
                c.cut(seg(28.0, 16.0, 28.0, 46.0, 3.5));
                c.cut(rect_c(32.5, 25.0, 51.0, 37.0, 2.0));
            });
            c.add(seg(10.0, 57.0, 54.0, 57.0, 4.5));
        }
        Expand => c.add(tri_rounded(&[(24.0, 17.0), (45.0, 32.0), (24.0, 47.0)])),
        Collapse => c.add(tri_rounded(&[(17.0, 24.0), (47.0, 24.0), (32.0, 45.0)])),
        Colony => {
            c.add(rect_c(12.0, 12.0, 52.0, 58.0, 4.0));
            c.knock(rect_c(23.0, 6.0, 41.0, 17.0, 3.0));
            c.cut(seg(20.0, 28.0, 44.0, 28.0, 4.0).or(seg(20.0, 38.0, 44.0, 38.0, 4.0)).or(seg(20.0, 48.0, 36.0, 48.0, 4.0)));
        }
        Jobs => {
            c.add(rect_c(22.0, 6.0, 42.0, 22.0, 3.0));
            c.add(seg(32.0, 22.0, 32.0, 40.0, 4.0));
            c.add(polyline(&[(13.0, 40.0), (13.0, 31.0), (51.0, 31.0), (51.0, 40.0)], 4.0));
            c.add(rect_c(5.0, 40.0, 21.0, 56.0, 3.0));
            c.add(rect_c(24.0, 40.0, 40.0, 56.0, 3.0));
            c.add(rect_c(43.0, 40.0, 59.0, 56.0, 3.0));
        }
        Journal => {
            c.add(poly(&[(5.0, 14.0), (29.0, 17.0), (29.0, 55.0), (5.0, 51.0)]).grow(1.5));
            c.add(poly(&[(35.0, 17.0), (59.0, 14.0), (59.0, 51.0), (35.0, 55.0)]).grow(1.5));
            c.cut(seg(10.0, 27.0, 24.0, 29.0, 3.0).or(seg(10.0, 36.0, 24.0, 38.0, 3.0)).or(seg(10.0, 45.0, 24.0, 47.0, 3.0)));
            c.cut(seg(40.0, 29.0, 54.0, 27.0, 3.0).or(seg(40.0, 38.0, 54.0, 36.0, 3.0)).or(seg(40.0, 47.0, 54.0, 45.0, 3.0)));
        }
        Log => {
            c.add(rect_c(16.0, 12.0, 48.0, 52.0, 1.0));
            c.cut(seg(22.0, 26.0, 42.0, 26.0, 3.5).or(seg(22.0, 34.0, 42.0, 34.0, 3.5)).or(seg(22.0, 42.0, 36.0, 42.0, 3.5)));
            c.knock(seg(11.0, 12.0, 49.0, 12.0, 9.0));
            c.knock(seg(15.0, 52.0, 53.0, 52.0, 9.0));
        }
        Follow => {
            c.add(circle(32.0, 52.0, 32.0).and(circle(32.0, 12.0, 32.0)).grow(1.0));
            c.cut(circle(32.0, 32.0, 12.5));
            c.add(circle(32.0, 32.0, 8.0));
        }
        Area => {
            for (x, y) in [(11.0, 13.0), (53.0, 13.0), (11.0, 51.0), (53.0, 51.0)] {
                c.add(rect(x, y, 5.5, 5.5, 1.5));
            }
            c.add(seg(23.0, 13.0, 41.0, 13.0, 4.5).or(seg(23.0, 51.0, 41.0, 51.0, 4.5)));
            c.add(seg(11.0, 25.0, 11.0, 39.0, 4.5).or(seg(53.0, 25.0, 53.0, 39.0, 4.5)));
        }
        Haul => {
            c.add(rect_c(8.0, 16.0, 44.0, 40.0, 3.0));
            c.add(seg(44.0, 22.0, 57.0, 10.0, W));
            c.knock(circle(18.0, 47.0, 7.0).or(circle(36.0, 47.0, 7.0)));
        }
        Menu => {
            for y in [16.0, 32.0, 48.0] {
                c.add(seg(12.0, y, 52.0, y, 7.0));
            }
        }
        Back => {
            c.add(seg(13.0, 32.0, 53.0, 32.0, 7.0));
            c.add(polyline(&[(30.0, 14.0), (12.0, 32.0), (30.0, 50.0)], 7.0));
        }
        Settings => {
            c.add(circle(32.0, 32.0, 19.0));
            c.add(radial(8, 32.0, 32.0, || rect(32.0, 10.0, 5.0, 6.0, 1.5)));
            c.cut(circle(32.0, 32.0, 8.0));
        }
        Beaver => head(c),
        Adult => c.part(32.0, 33.0, 0.98, 0.0, 0.0, head),
        Yearling => c.part(32.0, 37.0, 0.8, 0.0, 0.0, head),
        Kit => c.part(32.0, 41.0, 0.62, 0.0, 0.0, head),
        MoodGood => {
            face(c);
            c.cut(arc(32.0, 32.0, 13.0, 30.0, 150.0, 5.0));
        }
        MoodOk => {
            face(c);
            c.cut(seg(21.0, 41.0, 43.0, 41.0, 5.0));
        }
        MoodBad => {
            face(c);
            c.cut(arc(32.0, 54.0, 13.0, 220.0, 320.0, 5.0));
        }
        Health => c.add(heart()),
        Spring => {
            c.add(qbez((32.0, 56.0), (31.0, 42.0), (33.0, 30.0), 5.0));
            c.add(leaf((32.0, 34.0), (9.0, 22.0), 13.0));
            c.add(leaf((33.0, 30.0), (55.0, 12.0), 15.0));
            c.add(ellipse(32.0, 59.0, 20.0, 6.0).above(58.0));
        }
        Summer => sun(c, 13.0, 19.5, 27.0),
        Autumn => {
            c.part(32.0, 31.0, 1.0, 35.0, 0.0, |c| {
                c.add(leaf((32.0, 50.0), (32.0, 5.0), 40.0));
                c.add(seg(32.0, 48.0, 32.0, 61.0, 5.0));
                c.cut(seg(32.0, 18.0, 32.0, 50.0, 3.2));
                c.cut(seg(32.0, 36.0, 21.0, 27.0, 3.0).or(seg(32.0, 36.0, 43.0, 27.0, 3.0)));
            });
        }
        Winter => {
            c.add(radial(3, 32.0, 32.0, || seg(32.0, 6.0, 32.0, 58.0, 5.0)));
            c.add(radial(6, 32.0, 32.0, || polyline(&[(24.0, 11.0), (32.0, 19.0), (40.0, 11.0)], 4.5)));
        }
        DrySummer => {
            c.part(32.0, 23.0, 0.7, 0.0, 0.0, |c| sun(c, 13.0, 19.5, 27.0));
            c.add(rect_c(5.0, 46.0, 59.0, 58.0, 3.0));
            c.cut(polyline(&[(20.0, 44.0), (24.0, 51.0), (19.0, 60.0)], 3.0));
            c.cut(polyline(&[(42.0, 44.0), (38.0, 51.0), (44.0, 60.0)], 3.0));
        }
        Hourglass => {
            c.add(rect_c(12.0, 5.0, 52.0, 12.0, 2.5).or(rect_c(12.0, 52.0, 52.0, 59.0, 2.5)));
            c.add(poly(&[(17.0, 14.0), (47.0, 14.0), (34.5, 32.0), (47.0, 50.0), (17.0, 50.0), (29.5, 32.0)]).grow(1.0));
        }
        Walk => {
            c.part(20.0, 41.0, 0.62, -15.0, 0.0, paw);
            c.part(44.0, 22.0, 0.62, 15.0, 3.0, paw);
        }
        Tree => round_tree(c),
        Plant => {
            c.add(leaf((32.0, 57.0), (32.0, 6.0), 17.0));
            c.knock(leaf((30.0, 57.0), (5.0, 26.0), 13.0));
            c.knock(leaf((34.0, 57.0), (59.0, 26.0), 13.0));
        }
        MudBank => {
            let mut pts = vec![v(60.0, 58.0), v(18.0, 58.0)];
            pts.extend(qbez_pts(v(18.0, 58.0), v(30.0, 18.0), v(60.0, 16.0), 16));
            c.add(poly_v(pts));
            c.cut(circle(46.0, 34.0, 3.0).or(circle(52.0, 46.0, 2.5)).or(circle(38.0, 48.0, 2.5)));
            c.knock(wave(4.0, 30.0, 42.0, 2.5, 1.0, 4.5));
            c.knock(wave(4.0, 34.0, 53.0, 2.5, 1.0, 4.5));
        }
        Hammer => c.part(32.0, 32.0, 1.0, -40.0, 0.0, hammer),
        Balance => {
            c.add(seg(32.0, 12.0, 32.0, 52.0, 5.0));
            c.add(seg(18.0, 54.0, 46.0, 54.0, 5.0));
            c.add(seg(10.0, 18.0, 54.0, 18.0, 5.0));
            c.add(seg(14.0, 18.0, 8.0, 36.0, 3.0).or(seg(14.0, 18.0, 20.0, 36.0, 3.0)));
            c.add(seg(50.0, 18.0, 44.0, 36.0, 3.0).or(seg(50.0, 18.0, 56.0, 36.0, 3.0)));
            c.add(ellipse(14.0, 38.0, 8.0, 4.0).or(ellipse(50.0, 38.0, 8.0, 4.0)));
        }
        Crest => {
            c.add(seg(10.0, 8.0, 54.0, 8.0, W));
            c.add(seg(32.0, 22.0, 32.0, 48.0, W));
            c.add(tri_rounded(&[(32.0, 16.0), (43.0, 28.0), (21.0, 28.0)]));
            c.add(tri_rounded(&[(32.0, 56.0), (21.0, 44.0), (43.0, 44.0)]));
        }
        Rise => {
            c.add(seg(32.0, 22.0, 32.0, 42.0, 6.5));
            c.add(tri_rounded(&[(32.0, 7.0), (46.0, 24.0), (18.0, 24.0)]));
            c.add(wave(6.0, 58.0, 54.0, 3.5, 1.5, W));
        }
        Flood => {
            c.part(32.0, 26.0, 0.72, 0.0, 0.0, house);
            c.cut(rect_c(0.0, 36.0, 64.0, 64.0, 0.0));
            c.add(wave(4.0, 60.0, 40.0, 3.5, 2.0, W));
            c.add(wave(4.0, 60.0, 53.0, 3.5, 2.0, W));
        }
        Push => {
            c.add(rect_c(48.0, 6.0, 57.0, 58.0, 2.0));
            c.add(wave(6.0, 28.0, 32.0, 4.0, 1.0, 6.0));
            c.add(tri_rounded(&[(27.0, 20.0), (40.0, 32.0), (27.0, 44.0)]));
        }
        Drop => c.add(drop_shape().grow(1.5)),
        Site => {
            c.add(poly(&[(27.0, 8.0), (37.0, 8.0), (47.0, 50.0), (17.0, 50.0)]).grow(1.5));
            c.cut(seg(10.0, 22.0, 54.0, 22.0, 4.5).or(seg(10.0, 36.0, 54.0, 36.0, 4.5)));
            c.knock(rect_c(7.0, 50.0, 57.0, 57.0, 2.0));
        }
        Building => house_door(c),
        Home => {
            house(c);
            c.cut_part(32.0, 41.0, 0.42, 0.0, 0.0, |c| c.add(heart()));
        }
        Profile => {
            c.add(poly(&[(4.0, 58.0), (4.0, 24.0), (20.0, 24.0), (27.0, 38.0), (38.0, 38.0), (45.0, 48.0), (60.0, 48.0), (60.0, 58.0)]).grow(0.5));
            c.knock(wave(4.0, 22.0, 15.0, 2.0, 1.0, 4.5));
            c.knock(wave(28.0, 42.0, 30.0, 2.0, 1.0, 4.5));
        }
        Upstream => {
            c.add(wave(17.0, 57.0, 32.0, 4.5, 1.0, W + 0.5));
            c.add(polyline(&[(27.0, 15.0), (10.0, 32.0), (27.0, 49.0)], W + 0.5));
        }
        North => {
            c.add(tri_rounded(&[(32.0, 6.0), (44.0, 20.0), (20.0, 20.0)]));
            c.add(polyline(&[(19.0, 57.0), (19.0, 29.0), (45.0, 57.0), (45.0, 29.0)], W));
        }
        Gauge => {
            c.add(rect_c(12.0, 7.0, 40.0, 57.0, 8.0).outline(W));
            c.add(wave_fill(18.5, 33.5, 30.0, 51.0, 2.0, 1.0).and(rect_c(18.5, 13.0, 33.5, 50.5, 3.0)));
            c.add(seg(48.0, 16.0, 56.0, 16.0, 4.5).or(seg(48.0, 32.0, 54.0, 32.0, 4.5)).or(seg(48.0, 48.0, 56.0, 48.0, 4.5)));
        }
        Wheel => wheel(c),
        Sapling => seedling(c),
        TreeYoung => {
            c.add(seg(32.0, 38.0, 32.0, 58.0, 5.0));
            c.knock(circle(32.0, 27.0, 13.0).or(circle(23.0, 34.0, 8.0)).or(circle(41.0, 34.0, 8.0)));
            c.add(seg(20.0, 58.0, 44.0, 58.0, 4.0));
        }
        TreeOld => {
            c.add(poly(&[(25.0, 58.0), (28.0, 36.0), (36.0, 36.0), (40.0, 58.0)]).grow(1.0));
            c.add(seg(34.0, 40.0, 54.0, 22.0, 4.0));
            c.knock(circle(24.0, 22.0, 13.0).or(circle(12.0, 32.0, 8.0)).or(circle(37.0, 16.0, 9.0)));
            c.cut(circle(22.0, 21.0, 3.5).or(circle(31.0, 26.0, 2.8)).or(circle(13.0, 32.0, 2.5)));
        }
        Coppice => {
            c.add(rect_c(19.0, 40.0, 45.0, 57.0, 3.0));
            c.cut(ellipse(32.0, 40.0, 9.0, 3.0));
            c.add(qbez((25.0, 40.0), (22.0, 28.0), (12.0, 18.0), 4.0));
            c.add(qbez((32.0, 39.0), (32.0, 24.0), (32.0, 9.0), 4.0));
            c.add(qbez((39.0, 40.0), (42.0, 28.0), (52.0, 18.0), 4.0));
            c.add(leaf((12.0, 18.0), (9.0, 6.0), 6.0).or(leaf((32.0, 12.0), (24.0, 4.0), 6.0)).or(leaf((52.0, 18.0), (56.0, 6.0), 6.0)));
        }
    }
}

