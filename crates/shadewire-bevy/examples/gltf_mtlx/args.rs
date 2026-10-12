//! The command line: `[scene.glb|gltf] <material.mtlx> [--material NAME] [--screenshot out.png]`.

use std::path::PathBuf;

pub const USAGE: &str =
    "usage: gltf_mtlx [scene.glb|scene.gltf] <material.mtlx> [--material NAME] [--screenshot out.png]";

/// What the example was asked to show.
#[derive(Debug, PartialEq)]
pub struct Args {
    /// The glTF scene; `None` shows a sphere and a cube.
    pub scene: Option<PathBuf>,
    pub mtlx: PathBuf,
    /// The MaterialX material to build; the document's first one when `None`.
    pub material: Option<String>,
    /// Save a screenshot here after a few frames, then exit.
    pub screenshot: Option<PathBuf>,
}

/// Parses the arguments after the program name.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut material = None;
    let mut screenshot = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--material" => material = Some(args.next().ok_or("--material needs a name")?),
            "--screenshot" => screenshot = Some(PathBuf::from(args.next().ok_or("--screenshot needs a file")?)),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    let (scene, mtlx) = match positional.len() {
        1 => (None, positional.remove(0)),
        2 => {
            let mtlx = positional.remove(1);
            (Some(positional.remove(0)), mtlx)
        }
        n => return Err(format!("expected a .mtlx file and an optional scene, got {n} paths")),
    };
    Ok(Args { scene, mtlx, material, screenshot })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(s: &str) -> Result<Args, String> {
        parse(s.split_whitespace().map(String::from))
    }

    #[test]
    fn one_path_is_the_material() {
        let a = parse_str("m.mtlx").unwrap();
        assert_eq!(a, Args { scene: None, mtlx: "m.mtlx".into(), material: None, screenshot: None });
    }

    #[test]
    fn two_paths_are_scene_then_material_with_options_anywhere() {
        let a = parse_str("--screenshot s.png a.glb --material Marble m.mtlx").unwrap();
        assert_eq!(a.scene, Some("a.glb".into()));
        assert_eq!(a.mtlx, PathBuf::from("m.mtlx"));
        assert_eq!(a.material.as_deref(), Some("Marble"));
        assert_eq!(a.screenshot, Some("s.png".into()));
    }

    #[test]
    fn bad_command_lines_are_errors() {
        assert!(parse_str("").is_err());
        assert!(parse_str("a b c").is_err());
        assert!(parse_str("m.mtlx --material").is_err());
        assert!(parse_str("m.mtlx --frobnicate").is_err());
    }
}
