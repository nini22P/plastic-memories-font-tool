use std::collections::BTreeMap;

use ab_glyph::FontRef;
use image::{ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct AppConfig {
    pub output_dir: String,
    pub chars_file: String,
    pub font_sizes: Vec<FontSizeConfig>,
    #[serde(default = "default_atlas_size")]
    pub atlas_max_size: u32,
    #[serde(default)]
    pub global_offset_correction: f32,
    #[serde(default = "default_scale")]
    pub global_scale: f32,
    pub fonts: Vec<FontConfig>,
}

#[derive(Deserialize, Debug, Clone, Copy)]
#[allow(non_snake_case)]
pub struct FontSizeConfig {
    pub maxHeight: f32,
    pub maxWidth: f32,
    pub minHeight: f32,
    pub minWidth: f32,
}

#[derive(Deserialize, Debug)]
pub struct FontConfig {
    pub path: String,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default)]
    pub offset_y: f32,
}

fn default_atlas_size() -> u32 {
    2048
}
fn default_scale() -> f32 {
    1.0
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct PsbFont {
    pub version: f32,
    pub id: String,
    pub spec: String,
    pub label: String,
    pub minWidth: f32,
    pub minHeight: f32,
    pub maxWidth: f32,
    pub maxHeight: f32,
    pub source: Vec<PsbFontSource>,
    pub code: BTreeMap<String, CharEntry>,
}

#[derive(Serialize)]
pub struct PsbFontSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub pixel: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize)]
pub struct CharEntry {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub w: f32,
    pub h: f32,
    pub a: f32,
    pub b: f32,
    pub d: f32,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct PsbResx {
    pub PsbVersion: u32,
    pub PsbType: String,
    pub Platform: String,
    pub CryptKey: Option<String>,
    pub ExternalTextures: bool,
    pub Context: PsbResxContext,
    pub Resources: BTreeMap<String, String>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct PsbResxContext {
    pub MdfKeyLength: u32,
    pub FileName: String,
    pub MdfKey: String,
    pub PsbZlibFastCompress: bool,
    pub PsbShellType: String,
}

pub struct LoadedFont<'a> {
    pub font_ref: FontRef<'a>,
    pub config: &'a FontConfig,
    #[allow(dead_code)]
    pub data: Vec<u8>,
}

#[derive(Clone, Copy)]
pub struct FontMetrics {
    pub baseline_y: f32,
    pub param_a: f32,
    pub param_b: f32,
    pub param_d: f32,
}

pub struct CharData {
    pub c: char,
    pub img: Option<ImageBuffer<Rgba<u8>, Vec<u8>>>,
    pub w: u32,
    pub h: u32,
    pub advance: f32,
    pub offset_correction_y: f32,
}
