use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{ImageBuffer, Rgba};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

mod types;
use types::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config("config.json")?;
    let fonts = load_fonts(&config)?;
    let chars = load_characters(&config)?;

    println!("初始化完成。字符总数: {}", chars.len());

    fs::create_dir_all(&config.output_dir)?;

    for size_conf in &config.font_sizes {
        process_font_size(&fonts, size_conf, &chars, &config)?;
    }

    println!("\n全部任务完成！");
    Ok(())
}

fn process_font_size(
    fonts: &[LoadedFont],
    size_config: &FontSizeConfig,
    chars: &[char],
    global_config: &AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let base_size = size_config.maxHeight;
    let render_size = base_size * global_config.global_scale;

    println!(
        "正在生成字号: {:.0} (渲染尺寸: {:.1})...",
        base_size, render_size
    );

    let metrics = calculate_metrics(fonts, size_config, global_config, render_size);

    let mut char_data_list = Vec::new();
    for &c in chars {
        let char_data = rasterize_char(c, fonts, size_config, render_size, metrics.baseline_y);
        char_data_list.push(char_data);
    }

    pack_and_save(
        char_data_list,
        base_size,
        metrics,
        global_config,
        size_config,
    )?;

    Ok(())
}

fn calculate_metrics(
    fonts: &[LoadedFont],
    size_config: &FontSizeConfig,
    global_config: &AppConfig,
    render_size: f32,
) -> FontMetrics {
    let primary_font = &fonts[0];
    let scale = PxScale::from(render_size * primary_font.config.scale);
    let scaled_font = primary_font.font_ref.as_scaled(scale);

    let ascent = scaled_font.ascent();
    let fixed_height = (size_config.maxHeight.ceil().max(size_config.minHeight)) as u32;

    let vertical_center_offset = (fixed_height as f32 - render_size) / 2.0;
    let baseline_y = vertical_center_offset + ascent;

    let max_ref_size = global_config.font_sizes.last().unwrap().maxHeight;
    let size_ratio = size_config.maxHeight / max_ref_size;
    let dynamic_offset = (global_config.global_offset_correction * size_ratio).round();

    let unified_b = baseline_y + dynamic_offset;
    let unified_a = unified_b - fixed_height as f32;
    let unified_d = size_config.maxHeight;

    FontMetrics {
        baseline_y,
        param_a: unified_a,
        param_b: unified_b,
        param_d: unified_d,
    }
}

fn rasterize_char(
    c: char,
    fonts: &[LoadedFont],
    size_config: &FontSizeConfig,
    base_render_size: f32,
    drawing_baseline_y: f32,
) -> CharData {
    let font_idx = fonts
        .iter()
        .position(|lf| lf.font_ref.glyph_id(c).0 != 0)
        .unwrap_or(0);
    let lf = &fonts[font_idx];

    let actual_size = base_render_size * lf.config.scale;
    let scale = PxScale::from(actual_size);
    let scaled_font = lf.font_ref.as_scaled(scale);

    let glyph_id = lf.font_ref.glyph_id(c);
    let glyph = glyph_id.with_scale(scale);
    let h_advance = scaled_font.h_advance(glyph_id);
    let outlined = lf.font_ref.outline_glyph(glyph);

    let fixed_h = (size_config.maxHeight.ceil().max(size_config.minHeight)) as u32;

    let content_w = outlined
        .as_ref()
        .map(|g| g.px_bounds().width().ceil() as u32)
        .unwrap_or(0);
    let advance_w = h_advance.ceil() as u32;
    let img_w = advance_w.max(content_w).max(size_config.minWidth as u32);

    let mut shift_y = 0.0;

    let safety_margin = 0.0;

    if let Some(ref glyph_outline) = outlined {
        let bounds = glyph_outline.px_bounds();
        let manual_y_offset = lf.config.offset_y;

        let original_top_y = bounds.min.y + drawing_baseline_y + manual_y_offset;
        let original_bottom_y = bounds.max.y + drawing_baseline_y + manual_y_offset;

        if original_top_y < safety_margin {
            shift_y = safety_margin - original_top_y;
        } else if original_bottom_y > (fixed_h as f32 - safety_margin) {
            let shift_up = (fixed_h as f32 - safety_margin) - original_bottom_y;
            if original_top_y + shift_up >= 0.0 {
                shift_y = shift_up;
            } else {
                shift_y = safety_margin - original_top_y;
            }
        }
    }

    shift_y = shift_y.round();

    let mut img_buf = ImageBuffer::new(img_w, fixed_h);
    if let Some(glyph_outline) = outlined {
        let bounds = glyph_outline.px_bounds();
        let manual_y_offset = lf.config.offset_y;

        glyph_outline.draw(|x, y, coverage| {
            let alpha = (coverage * 255.0) as u8;
            if alpha == 0 {
                return;
            }

            let dest_x = bounds.min.x + x as f32;

            let dest_y = (bounds.min.y + y as f32) + drawing_baseline_y + manual_y_offset + shift_y;

            if dest_x >= 0.0 && dest_x < img_w as f32 && dest_y >= 0.0 && dest_y < fixed_h as f32 {
                img_buf.put_pixel(dest_x as u32, dest_y as u32, Rgba([255, 255, 255, alpha]));
            }
        });
    }

    let final_advance = h_advance.max(size_config.minWidth);

    CharData {
        c,
        img: Some(img_buf),
        w: img_w,
        h: fixed_h,
        advance: final_advance,
        offset_correction_y: shift_y,
    }
}

fn pack_and_save(
    char_list: Vec<CharData>,
    base_size: f32,
    metrics: FontMetrics,
    global_config: &AppConfig,
    size_config: &FontSizeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let atlas_size = global_config.atlas_max_size;
    let mut atlases = Vec::new();
    let mut current_atlas = ImageBuffer::new(atlas_size, atlas_size);

    let mut cursor_x = 0;
    let mut cursor_y = 0;
    let mut max_row_h = 0;
    let mut max_w_used = 0;
    let mut max_h_used = 0;
    let mut current_idx = 0;

    let mut json_code_map = BTreeMap::new();

    for item in &char_list {
        if cursor_x + item.w > atlas_size {
            cursor_x = 0;
            cursor_y += max_row_h;
            max_row_h = 0;
        }

        if cursor_y + item.h > atlas_size {
            let w = next_power_of_two(max_w_used);
            let h = next_power_of_two(max_h_used);
            atlases.push(image::imageops::crop_imm(&current_atlas, 0, 0, w, h).to_image());

            current_atlas = ImageBuffer::new(atlas_size, atlas_size);
            current_idx += 1;
            cursor_x = 0;
            cursor_y = 0;
            max_w_used = 0;
            max_h_used = 0;
            max_row_h = 0;
        }

        let (final_x, final_y) = if let Some(ref glyph_img) = item.img {
            image::imageops::overlay(
                &mut current_atlas,
                glyph_img,
                cursor_x as i64,
                cursor_y as i64,
            );
            (cursor_x as f32, cursor_y as f32)
        } else {
            (1.0, 1.0)
        };

        if cursor_x + item.w > max_w_used {
            max_w_used = cursor_x + item.w;
        }
        if cursor_y + item.h > max_h_used {
            max_h_used = cursor_y + item.h;
        }
        if item.h > max_row_h {
            max_row_h = item.h;
        }

        cursor_x += item.w;

        let corrected_a = metrics.param_a + item.offset_correction_y;
        let corrected_b = metrics.param_b + item.offset_correction_y;

        json_code_map.insert(
            item.c.to_string(),
            CharEntry {
                id: current_idx,
                x: final_x.round(),
                y: final_y.round(),
                width: item.advance.round(),
                height: base_size.round(),
                w: item.w as f32,
                h: item.h as f32,
                a: corrected_a.round(),
                b: corrected_b.round(),
                d: metrics.param_d.round(),
            },
        );
    }

    let w = next_power_of_two(max_w_used).max(64);
    let h = next_power_of_two(max_h_used).max(64);
    atlases.push(image::imageops::crop_imm(&current_atlas, 0, 0, w, h).to_image());

    write_output_files(
        base_size,
        atlases,
        json_code_map,
        global_config,
        size_config,
    )?;

    Ok(())
}

fn write_output_files(
    base_size: f32,
    atlases: Vec<image::RgbaImage>,
    code_map: BTreeMap<String, CharEntry>,
    global_config: &AppConfig,
    size_config: &FontSizeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let base_name = format!("textfont{:.0}.psb.m", base_size);
    let out_dir_path = Path::new(&global_config.output_dir).join(&base_name);
    fs::create_dir_all(&out_dir_path)?;

    let mut resources = BTreeMap::new();
    let mut sources = Vec::new();

    for (i, atlas) in atlases.iter().enumerate() {
        let filename = format!("[{}]-[{}].png", i, i);
        let file_path = out_dir_path.join(&filename);
        atlas.save(&file_path)?;

        resources.insert(i.to_string(), format!("{}/{}", base_name, filename));
        sources.push(PsbFontSource {
            source_type: "A8_SW".to_string(),
            pixel: format!("#resource#{}", i),
            width: atlas.width(),
            height: atlas.height(),
        });
    }

    let font_json = PsbFont {
        version: 1.08,
        id: "font".to_string(),
        spec: "vita".to_string(),
        label: "normal".to_string(),
        minWidth: size_config.minWidth.round(),
        minHeight: size_config.minHeight.round(),
        maxWidth: size_config.maxWidth.round(),
        maxHeight: size_config.maxHeight.round(),
        source: sources,
        code: code_map,
    };
    fs::write(
        Path::new(&global_config.output_dir).join(format!("{}.json", base_name)),
        serde_json::to_string_pretty(&font_json)?,
    )?;

    let mdf_key = format!("2shj693vwue5t{}", base_name);
    let resx_json = PsbResx {
        PsbVersion: 2,
        PsbType: "BmpFont".to_string(),
        Platform: "vita".to_string(),
        CryptKey: None,
        ExternalTextures: false,
        Context: PsbResxContext {
            MdfKeyLength: 131,
            FileName: base_name.clone(),
            MdfKey: mdf_key,
            PsbZlibFastCompress: false,
            PsbShellType: "MDF".to_string(),
        },
        Resources: resources,
    };
    fs::write(
        Path::new(&global_config.output_dir).join(format!("{}.resx.json", base_name)),
        serde_json::to_string_pretty(&resx_json)?,
    )?;

    println!("  -> 输出文件: {}.resx.json", base_name);
    Ok(())
}

fn load_config(path: &str) -> Result<AppConfig, Box<dyn std::error::Error>> {
    if !Path::new(path).exists() {
        return Err(format!("配置文件不存在: {}", path).into());
    }
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn load_fonts<'a>(
    config: &'a AppConfig,
) -> Result<Vec<LoadedFont<'a>>, Box<dyn std::error::Error>> {
    let mut fonts = Vec::new();
    for font_conf in &config.fonts {
        println!("加载字体文件: {}", font_conf.path);
        let data =
            fs::read(&font_conf.path).map_err(|_| format!("无法读取: {}", font_conf.path))?;
        let font_ref = FontRef::try_from_slice(unsafe {
            std::slice::from_raw_parts(data.as_ptr(), data.len())
        })?;
        fonts.push(LoadedFont {
            font_ref,
            config: font_conf,
            data,
        });
    }
    if fonts.is_empty() {
        return Err("配置文件中没有定义任何字体".into());
    }
    Ok(fonts)
}

fn load_characters(config: &AppConfig) -> Result<Vec<char>, Box<dyn std::error::Error>> {
    let content = if Path::new(&config.chars_file).exists() {
        fs::read_to_string(&config.chars_file)?
    } else {
        println!("警告: 未找到字符集文件，使用默认备用字符集");
        " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{{|}}~".to_string()
    };
    let mut unique: Vec<char> = content
        .chars()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    unique.sort();
    if let Some(pos) = unique.iter().position(|&c| c == ' ') {
        unique.remove(pos);
    }
    unique.insert(0, ' ');
    Ok(unique)
}

fn next_power_of_two(v: u32) -> u32 {
    let mut p = 64;
    while p < v {
        p *= 2;
    }
    p
}
