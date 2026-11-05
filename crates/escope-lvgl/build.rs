use std::{env, fmt::Write as _, fs, path::PathBuf};

// Public constructor suffix and inspection kind. Only classes present in the
// effective target bindings are referenced, including reduced configurations.
const WIDGETS: &[(&str, &str)] = &[
    ("label", "label"),
    ("button", "button"),
    ("animimg", "animimage"),
    ("arc", "arc"),
    ("arclabel", "arclabel"),
    ("bar", "bar"),
    ("barcode", "barcode"),
    ("buttonmatrix", "buttonmatrix"),
    ("calendar", "calendar"),
    ("canvas", "canvas"),
    ("chart", "chart"),
    ("checkbox", "checkbox"),
    ("dropdown", "dropdown"),
    ("gif", "gif"),
    ("image", "image"),
    ("imagebutton", "imagebutton"),
    ("ime_pinyin", "pinyin_keyboard"),
    ("keyboard", "keyboard"),
    ("led", "led"),
    ("line", "line"),
    ("list", "list"),
    ("lottie", "lottie"),
    ("menu", "menu"),
    ("msgbox", "msgbox"),
    ("qrcode", "qrcode"),
    ("roller", "roller"),
    ("scale", "scale"),
    ("slider", "slider"),
    ("spangroup", "spans"),
    ("spinbox", "spinbox"),
    ("spinner", "spinner"),
    ("switch", "switch"),
    ("table", "table"),
    ("tabview", "tabview"),
    ("textarea", "textarea"),
    ("tileview", "tileview"),
    ("win", "win"),
];

fn main() {
    println!("cargo:rerun-if-env-changed=DEP_LVGL_BINDINGS");
    if env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }
    let path = env::var("DEP_LVGL_BINDINGS").expect("native LVGL provider bindings");
    println!("cargo:rerun-if-changed={path}");
    let bindings = fs::read_to_string(path).expect("read native LVGL provider bindings");
    let widgets: Vec<_> = WIDGETS
        .iter()
        .filter(|(symbol, _)| bindings.contains(&format!("pub fn lv_{symbol}_create(")))
        .collect();

    let mut code =
        String::from("unsafe extern \"C\" {\nstatic lv_obj_class: sys::lv_obj_class_t;\n");
    for (symbol, _) in &widgets {
        writeln!(code, "static lv_{symbol}_class: sys::lv_obj_class_t;").unwrap();
    }
    code.push_str(
        "}\nunsafe fn object_kind(object: *mut sys::lv_obj_t) -> &'static str {\nunsafe {\n",
    );
    for (symbol, kind) in widgets {
        writeln!(
            code,
            "if sys::lv_obj_check_type(object, &raw const lv_{symbol}_class) {{ return {kind:?}; }}"
        )
        .unwrap();
    }
    code.push_str("if sys::lv_obj_check_type(object, &raw const lv_obj_class) {\n");
    // Layout enum values depend on LV_USE_FLEX/GRID. Use only the constants
    // exposed by this provider instead of assuming their numeric values.
    if bindings.contains("pub const lv_layout_t_LV_LAYOUT_FLEX:") {
        code.push_str(
            "if u32::from(sys::lv_obj_get_style_layout(object, 0)) == sys::lv_layout_t_LV_LAYOUT_FLEX {\n\
             return if sys::lv_obj_get_style_flex_flow(object, 0) & sys::lv_flex_flow_t_LV_FLEX_FLOW_COLUMN != 0 {\n\
             \"column\"\n} else {\n\"row\"\n};\n}\n",
        );
    }
    if bindings.contains("pub const lv_layout_t_LV_LAYOUT_GRID:") {
        code.push_str(
            "if u32::from(sys::lv_obj_get_style_layout(object, 0)) == sys::lv_layout_t_LV_LAYOUT_GRID { return \"grid\"; }\n",
        );
    }
    code.push_str("}\n\"lv_obj\"\n}\n}\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("native_kind.rs");
    fs::write(output, code).expect("write native inspection kinds");
}
