//! CJK fallback from the AppImage bundle or installed system fonts.
use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::path::PathBuf;

const FALLBACK: &str = "system-cjk";

pub fn install_system_cjk(ctx: &egui::Context) {
    if let Some((fonts, _)) = from_candidates(candidates()) {
        ctx.set_fonts(fonts);
    }
}

fn from_candidates(paths: impl IntoIterator<Item = PathBuf>) -> Option<(FontDefinitions, PathBuf)> {
    for path in paths {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }
        let mut fonts = FontDefinitions::default();
        fonts
            .font_data
            .insert(FALLBACK.into(), FontData::from_owned(bytes).into());
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push(FALLBACK.into());
        }
        return Some((fonts, path));
    }
    None
}

fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "macos")]
    {
        paths.extend([
            PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
            PathBuf::from("/Library/Fonts/PingFang.ttc"),
        ]);
        // Newer macOS versions keep PingFang in an installed font asset.
        // Enumerate only that catalogue and its immediate assets, never fetch it.
        let mut assets = Vec::new();
        if let Ok(catalogues) = std::fs::read_dir("/System/Library/AssetsV2") {
            for catalogue in catalogues.flatten() {
                if !catalogue
                    .file_name()
                    .to_string_lossy()
                    .starts_with("com_apple_MobileAsset_Font")
                {
                    continue;
                }
                if let Ok(entries) = std::fs::read_dir(catalogue.path()) {
                    for entry in entries.flatten() {
                        assets.push(entry.path().join("AssetData/PingFang.ttc"));
                    }
                }
            }
        }
        assets.sort();
        paths.extend(assets);
        paths.extend([
            PathBuf::from("/System/Library/Fonts/Hiragino Sans GB.ttc"),
            PathBuf::from("/System/Library/Fonts/STHeiti Light.ttc"),
            PathBuf::from("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"),
        ]);
    }
    #[cfg(target_os = "windows")]
    {
        let root = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
        let directory = PathBuf::from(root).join("Fonts");
        paths.extend(
            ["msyh.ttc", "msyh.ttf", "simhei.ttf", "simsun.ttc"].map(|name| directory.join(name)),
        );
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(appdir) = std::env::var_os("APPDIR") {
            paths.push(PathBuf::from(appdir).join("usr/share/fonts/NotoSansCJK-Regular.ttc"));
        }
        paths.extend(
            [
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/google-noto-sans-cjk-vf-fonts/NotoSansCJK-VF.otf.ttc",
                "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
                "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            ]
            .map(PathBuf::from),
        );
        if let Some(home_directory) = std::env::var_os("HOME") {
            let home_directory = PathBuf::from(home_directory);
            paths.push(home_directory.join(".local/share/fonts/NotoSansCJK-Regular.ttc"));
            paths.push(home_directory.join(".fonts/NotoSansCJK-Regular.ttc"));
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_optional_font_keeps_the_default_configuration() {
        let missing =
            std::env::temp_dir().join(format!("escope-absent-font-{}.ttf", std::process::id()));
        assert!(from_candidates([missing]).is_none());
    }

    #[test]
    fn installed_system_fallback_has_chinese_glyphs_in_both_families() {
        let Some((fonts, path)) = from_candidates(candidates()) else {
            // A machine without optional CJK fonts still starts normally.
            return;
        };
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            ui.ctx().fonts_mut(|fonts| {
                for family in [FontFamily::Proportional, FontFamily::Monospace] {
                    let id = egui::FontId::new(14.0, family);
                    assert!(fonts.has_glyphs(&id, "设置返回应用"), "{}", path.display());
                }
            });
        });
        output.textures_delta.clear();
    }
}
