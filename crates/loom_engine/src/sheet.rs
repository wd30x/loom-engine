//! Sprite sheets of a scene's sprites and tilemaps: Aseprite `json-array`
//! exports, read and checked when the scene is loaded, whether or not it is
//! rendered, and uploaded to the renderer on first draw
//! (`docs/architecture.md` section 14).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use loom_render::{RenderError, Renderer, TextureId};
use loom_scene::Scene;
use serde::Deserialize;

/// The fields of an Aseprite `json-array` export that are used. It is an
/// external format: other fields are ignored.
#[derive(Deserialize)]
struct SheetFile {
    frames: Vec<FrameEntry>,
    meta: Meta,
}

#[derive(Deserialize)]
struct FrameEntry {
    frame: Rect,
}

#[derive(Deserialize)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[derive(Deserialize)]
struct Meta {
    /// PNG path relative to the JSON file's directory.
    image: String,
    size: SizeEntry,
    #[serde(rename = "frameTags", default)]
    frame_tags: Vec<TagEntry>,
}

#[derive(Deserialize)]
struct SizeEntry {
    w: u32,
    h: u32,
}

#[derive(Deserialize)]
struct TagEntry {
    name: String,
    from: usize,
    to: usize,
}

/// One checked sprite sheet: its pixels, frames and tags.
pub(crate) struct SpriteSheet {
    width: u32,
    height: u32,
    /// Tightly packed sRGB RGBA8 rows, top row first.
    rgba: Vec<u8>,
    /// x, y, width, height of each frame in pixels; non-empty and inside
    /// the image.
    frames: Vec<[u32; 4]>,
    /// Name and first frame (`from`) of each tag, in file order; names are
    /// unique and every `from` indexes `frames`.
    tags: Vec<(String, usize)>,
    /// The uploaded texture, once drawn.
    texture: Option<TextureId>,
}

impl SpriteSheet {
    /// Reads the JSON at `path` and the PNG it names, and checks them.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).context("reading the JSON")?;
        let file: SheetFile = serde_json::from_str(&text).context("parsing the JSON")?;
        let dir = path.parent().unwrap_or(Path::new(""));
        let image_path = dir.join(&file.meta.image);
        let image = image::open(&image_path)
            .with_context(|| format!("reading image {}", image_path.display()))?
            .to_rgba8();
        let (width, height) = image.dimensions();
        let size = &file.meta.size;
        ensure!(
            (width, height) == (size.w, size.h),
            "image {} is {width}x{height}, but meta.size says {}x{}",
            image_path.display(),
            size.w,
            size.h
        );
        let frames: Vec<[u32; 4]> = file
            .frames
            .iter()
            .map(|f| [f.frame.x, f.frame.y, f.frame.w, f.frame.h])
            .collect();
        for (i, &[x, y, w, h]) in frames.iter().enumerate() {
            let inside = u64::from(x) + u64::from(w) <= u64::from(width)
                && u64::from(y) + u64::from(h) <= u64::from(height);
            ensure!(
                w > 0 && h > 0 && inside,
                "frame {i} ({w}x{h} at {x},{y}) is empty or outside the {width}x{height} image"
            );
        }
        let mut tags: Vec<(String, usize)> = Vec::with_capacity(file.meta.frame_tags.len());
        for tag in &file.meta.frame_tags {
            ensure!(
                !tags.iter().any(|(name, _)| *name == tag.name),
                "tag {:?} is listed more than once",
                tag.name
            );
            ensure!(
                tag.from <= tag.to && tag.to < frames.len(),
                "tag {:?} spans frames {} to {}, but needs from <= to < {} (the frame count)",
                tag.name,
                tag.from,
                tag.to,
                frames.len()
            );
            tags.push((tag.name.clone(), tag.from));
        }
        Ok(Self {
            width,
            height,
            rgba: image.into_raw(),
            frames,
            tags,
            texture: None,
        })
    }

    /// The pixel rectangle of `tag`'s first frame, if the tag exists.
    pub fn rect(&self, tag: &str) -> Option<[u32; 4]> {
        let &(_, from) = self.tags.iter().find(|(name, _)| name == tag)?;
        Some(self.frames[from])
    }
}

/// Every sheet a scene uses, by path as written in the scene.
#[derive(Default)]
pub(crate) struct Sheets {
    sheets: BTreeMap<String, SpriteSheet>,
}

impl Sheets {
    /// Reads each sheet the scene's sprites and tilemaps use (once per path,
    /// relative to the working directory) and checks that every tag they
    /// use exists. Errors name the node, e.g. `node map: tilemap: sheet
    /// assets/art/tiles.json has no tag "flor"`.
    pub fn load(scene: &Scene) -> Result<Self> {
        let mut sheets = BTreeMap::new();
        for &id in scene.sprite_nodes() {
            let node = scene.node(id);
            let (component, sheet, tags): (&str, &str, Vec<&str>) =
                match (&node.sprite, &node.tilemap) {
                    // Every declared tag, alternates included: the game may
                    // switch to any of them while it runs.
                    (Some(s), _) => (
                        "sprite",
                        &s.sheet,
                        s.tags().iter().map(String::as_str).collect(),
                    ),
                    (None, Some(t)) => (
                        "tilemap",
                        &t.sheet,
                        t.legend.iter().map(|(_, tag)| tag.as_str()).collect(),
                    ),
                    (None, None) => unreachable!("Scene::sprite_nodes has a sprite or tilemap"),
                };
            let node_context = || format!("node {}: {component}", scene.path(id));
            if !sheets.contains_key(sheet) {
                let loaded = SpriteSheet::load(Path::new(sheet))
                    .with_context(|| format!("sheet {sheet}"))
                    .with_context(node_context)?;
                sheets.insert(sheet.to_owned(), loaded);
            }
            let loaded = &sheets[sheet];
            for tag in tags {
                if loaded.rect(tag).is_none() {
                    bail!("{}: sheet {sheet} has no tag {tag:?}", node_context());
                }
            }
        }
        Ok(Self { sheets })
    }

    /// The texture of `sheet` and the rectangle of `tag`'s first frame,
    /// uploading the sheet to `renderer` on first use. A `Sheets` must only
    /// ever draw with one renderer.
    ///
    /// Panics if `sheet` or `tag` was not checked by [`Sheets::load`] for
    /// the scene being drawn.
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        sheet: &str,
        tag: &str,
    ) -> Result<(TextureId, [u32; 4]), RenderError> {
        let loaded = self
            .sheets
            .get_mut(sheet)
            .expect("Sheets::load read every sheet of the scene");
        let rect = loaded
            .rect(tag)
            .expect("Sheets::load checked every tag of the scene");
        let texture = match loaded.texture {
            Some(texture) => texture,
            None => {
                let texture = renderer.create_texture(loaded.width, loaded.height, &loaded.rgba)?;
                loaded.texture = Some(texture);
                texture
            }
        };
        Ok((texture, rect))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A fresh directory for one test.
    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("loom_sheet_tests_{}", std::process::id()))
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creating a temp dir");
        dir
    }

    /// Writes an opaque `width` x `height` `sheet.png` and a `sheet.json`
    /// with `meta.size` `size`, the given frames and tags, into `dir`.
    fn write_sheet(
        dir: &Path,
        width: u32,
        height: u32,
        size: (u32, u32),
        frames: &[[u32; 4]],
        tags: &[(&str, usize, usize)],
    ) -> PathBuf {
        image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]))
            .save(dir.join("sheet.png"))
            .expect("writing the test PNG");
        let frames: Vec<_> = frames
            .iter()
            .map(|[x, y, w, h]| serde_json::json!({ "frame": { "x": x, "y": y, "w": w, "h": h } }))
            .collect();
        let tags: Vec<_> = tags
            .iter()
            .map(|(name, from, to)| serde_json::json!({ "name": name, "from": from, "to": to }))
            .collect();
        let json = serde_json::json!({
            "frames": frames,
            "meta": { "image": "sheet.png", "size": { "w": size.0, "h": size.1 }, "frameTags": tags },
        });
        let path = dir.join("sheet.json");
        std::fs::write(&path, json.to_string()).expect("writing the test JSON");
        path
    }

    const TWO_FRAMES: [[u32; 4]; 2] = [[0, 0, 8, 8], [8, 0, 8, 8]];

    fn load_error(path: &Path) -> String {
        match SpriteSheet::load(path) {
            Ok(_) => panic!("{} should be rejected", path.display()),
            Err(e) => format!("{e:#}"),
        }
    }

    #[test]
    fn valid_test_sheet_loads_with_its_first_frames() {
        let dir = test_dir("valid");
        let path = write_sheet(
            &dir,
            16,
            8,
            (16, 8),
            &TWO_FRAMES,
            &[("a", 0, 1), ("b", 1, 1)],
        );
        let sheet = SpriteSheet::load(&path).expect("a valid sheet");
        assert_eq!(
            (sheet.width, sheet.height, sheet.rgba.len()),
            (16, 8, 16 * 8 * 4)
        );
        assert_eq!(sheet.rect("a"), Some([0, 0, 8, 8]));
        assert_eq!(sheet.rect("b"), Some([8, 0, 8, 8]));
        assert_eq!(sheet.rect("c"), None);
    }

    #[test]
    fn duplicate_tag_names_are_rejected() {
        let dir = test_dir("duplicate");
        let path = write_sheet(
            &dir,
            16,
            8,
            (16, 8),
            &TWO_FRAMES,
            &[("a", 0, 0), ("a", 1, 1)],
        );
        assert!(load_error(&path).contains("tag \"a\" is listed more than once"));
    }

    #[test]
    fn tag_starting_after_its_end_is_rejected() {
        let dir = test_dir("reversed");
        let path = write_sheet(&dir, 16, 8, (16, 8), &TWO_FRAMES, &[("a", 1, 0)]);
        assert!(load_error(&path).contains("tag \"a\" spans frames 1 to 0"));
    }

    #[test]
    fn tag_ending_past_the_last_frame_is_rejected() {
        let dir = test_dir("past_end");
        let path = write_sheet(&dir, 16, 8, (16, 8), &TWO_FRAMES, &[("a", 1, 2)]);
        let error = load_error(&path);
        assert!(error.contains("tag \"a\" spans frames 1 to 2"), "{error}");
        assert!(error.contains("< 2 (the frame count)"), "{error}");
    }

    #[test]
    fn frame_outside_the_image_is_rejected() {
        let dir = test_dir("outside");
        let path = write_sheet(&dir, 16, 8, (16, 8), &[[0, 0, 8, 8], [9, 0, 8, 8]], &[]);
        let error = load_error(&path);
        assert!(error.contains("frame 1 (8x8 at 9,0)"), "{error}");
        assert!(error.contains("outside the 16x8 image"), "{error}");
    }

    #[test]
    fn empty_frame_is_rejected() {
        let dir = test_dir("empty_frame");
        let path = write_sheet(&dir, 16, 8, (16, 8), &[[0, 0, 0, 8]], &[]);
        assert!(load_error(&path).contains("frame 0 (0x8 at 0,0) is empty"));
    }

    #[test]
    fn image_size_must_match_meta_size() {
        let dir = test_dir("size_mismatch");
        let path = write_sheet(&dir, 16, 8, (32, 8), &TWO_FRAMES, &[]);
        let error = load_error(&path);
        assert!(
            error.contains("is 16x8, but meta.size says 32x8"),
            "{error}"
        );
    }

    #[test]
    fn missing_image_is_rejected_with_its_path() {
        let dir = test_dir("missing_image");
        let path = write_sheet(&dir, 16, 8, (16, 8), &TWO_FRAMES, &[]);
        std::fs::remove_file(dir.join("sheet.png")).expect("removing the test PNG");
        assert!(load_error(&path).contains("sheet.png"));
    }

    #[test]
    fn project_tile_sheet_decodes_to_rgba_with_transparency() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/art/tiles.json");
        let sheet = SpriteSheet::load(&path).unwrap_or_else(|e| {
            panic!(
                "reading {} failed (check git lfs pull): {e:#}",
                path.display()
            )
        });
        let names: Vec<(&str, usize)> = sheet.tags.iter().map(|(n, f)| (n.as_str(), *f)).collect();
        assert_eq!(
            names,
            [
                ("floor", 0),
                ("wall", 1),
                ("player", 2),
                ("zombie", 3),
                ("corpse", 4)
            ]
        );
        assert_eq!((sheet.width, sheet.height), (80, 16));
        assert_eq!(sheet.rgba.len(), 80 * 16 * 4);
        assert_eq!(sheet.rect("corpse"), Some([64, 0, 16, 16]));
        let alpha = |x: u32, y: u32| sheet.rgba[((y * sheet.width + x) * 4 + 3) as usize];
        let [fx, fy, ..] = sheet.rect("floor").expect("floor tag");
        assert_eq!(alpha(fx, fy), 255, "floor's top-left pixel is opaque");
        let [px, py, pw, ph] = sheet.rect("player").expect("player tag");
        let transparent = (py..py + ph).any(|y| (px..px + pw).any(|x| alpha(x, y) == 0));
        assert!(
            transparent,
            "the player frame has transparent (index 0) pixels"
        );
    }
}
