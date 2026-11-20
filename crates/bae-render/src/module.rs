//! [`RenderModule`]: the renderer's app module. Implements `bae render`:
//!
//! ```text
//! bae render <PROJECT> --comp <PATH|NAME> [--frame N | --time S | --frames A-B|all]
//!            [-o FILE|PATTERN] [--resolution full|half|third|quarter|N] [--depth 8|16|32]
//!            [--alpha straight|premultiplied|background] [--roi X,Y,W,H] [--mock-eval] [--json]
//! ```
//!
//! Without a window, on the shared GPU context; for CI, agents and render farms.
//! Sequences use `#` runs in the output pattern for the frame number
//! (`out_####.png`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use bae_app::{AppBuilder, AppModule};
use bae_gpu::{GpuContext, GpuContextDescriptor};
use bae_model::{FrameRate, PrimPath, PrimType, Project, Stage, Time, Value};
use bae_platform::cli::{self, CliSubcommand};
use clap::{Arg, ArgAction, ArgMatches};

use crate::anim::AnimEvaluator;
use crate::eval::{CompEvaluator, MockEvaluator, sample};
use crate::geom::IRect;
use crate::renderer::{OutputAlpha, Renderer, RendererConfig};
use crate::request::{BitDepth, RenderPurpose, RenderRequest, Resolution};

/// Registers `bae render`.
#[derive(Debug, Default)]
pub struct RenderModule;

impl AppModule for RenderModule {
    fn name(&self) -> &str {
        "bae-render"
    }

    fn build(&self, app: &mut AppBuilder) {
        // The GPU module provides the context descriptor (and the placeholder).
        app.add_module(bae_gpu::GpuModule);
        let registry = app.registry::<CliSubcommand>();
        let render = CliSubcommand::new("render", "Render a composition without a window (PNG)")
            .with_args(args)
            .with_run(|ctx| {
                let desc = ctx
                    .app
                    .resource::<GpuContextDescriptor>()
                    .cloned()
                    .unwrap_or_default();
                run(ctx.matches, desc)
            });
        if let Err(error) = cli::implement(&registry, app.current_owner(), render) {
            tracing::warn!("render not registered: {error}");
        }
    }
}

fn args(cmd: clap::Command) -> clap::Command {
    cmd.arg(
        Arg::new("project")
            .value_name("PROJECT")
            .required(true)
            .value_parser(clap::value_parser!(PathBuf))
            .help("A .bae directory or .baez file"),
    )
    .arg(
        Arg::new("comp")
            .long("comp")
            .short('c')
            .value_name("PATH")
            .required(true)
            .help("Comp prim path (/comps/main) or name under /comps (main)"),
    )
    .arg(
        Arg::new("frame")
            .long("frame")
            .value_name("N")
            .value_parser(clap::value_parser!(i64))
            .conflicts_with_all(["time", "frames"])
            .help("Comp frame number (default 0)"),
    )
    .arg(
        Arg::new("time")
            .long("time")
            .value_name("SECONDS")
            .value_parser(clap::value_parser!(f64))
            .conflicts_with("frames")
            .help("Comp time in seconds"),
    )
    .arg(
        Arg::new("frames")
            .long("frames")
            .value_name("A-B|all")
            .help("Frame range (inclusive) or `all` (the comp duration)"),
    )
    .arg(
        Arg::new("out")
            .long("out")
            .short('o')
            .value_name("FILE")
            .value_parser(clap::value_parser!(PathBuf))
            .help("Output PNG; for sequences a pattern with #### (default: <comp>[_####].png)"),
    )
    .arg(
        Arg::new("resolution")
            .long("resolution")
            .value_name("RES")
            .default_value("full")
            .help("full | half | third | quarter | N (every Nth pixel)"),
    )
    .arg(
        Arg::new("depth")
            .long("depth")
            .value_name("BPC")
            .default_value("8")
            .value_parser(["8", "16", "32"])
            .help("Project colour depth semantics (8/16 clamp like AE, 32 float)"),
    )
    .arg(
        Arg::new("alpha")
            .long("alpha")
            .value_name("MODE")
            .default_value("straight")
            .value_parser(["straight", "premultiplied", "background"])
            .help("PNG alpha: straight, premultiplied, or opaque over the comp background"),
    )
    .arg(
        Arg::new("roi")
            .long("roi")
            .value_name("X,Y,W,H")
            .help("Render only this region (comp pixels)"),
    )
    .arg(
        Arg::new("mock-eval")
            .long("mock-eval")
            .action(ArgAction::SetTrue)
            .help("Use the renderer's mock evaluator instead of bae-anim"),
    )
    .arg(
        Arg::new("json")
            .long("json")
            .action(ArgAction::SetTrue)
            .help("Print a machine-readable JSON report"),
    )
}

fn comp_path(arg: &str) -> anyhow::Result<PrimPath> {
    let s = if arg.starts_with('/') {
        arg.to_owned()
    } else {
        format!("/comps/{arg}")
    };
    Ok(PrimPath::new(&s)?)
}

fn parse_resolution(s: &str) -> anyhow::Result<Resolution> {
    Ok(match s {
        "full" | "1" => Resolution::Full,
        "half" | "2" => Resolution::Half,
        "third" | "3" => Resolution::Third,
        "quarter" | "4" => Resolution::Quarter,
        n => Resolution::Custom(
            n.parse()
                .map_err(|_| anyhow::anyhow!("bad --resolution `{n}`"))?,
        ),
    })
}

fn parse_roi(s: &str) -> anyhow::Result<IRect> {
    let v: Vec<i32> = s
        .split(',')
        .map(|p| p.trim().parse())
        .collect::<Result<_, _>>()
        .map_err(|_| anyhow::anyhow!("bad --roi `{s}` (X,Y,W,H)"))?;
    anyhow::ensure!(
        v.len() == 4 && v[2] > 0 && v[3] > 0,
        "bad --roi `{s}` (X,Y,W,H)"
    );
    Ok(IRect::new(v[0], v[1], v[0] + v[2], v[1] + v[3]))
}

/// Frame rate and duration of a comp prim.
fn comp_timing(stage: &Stage, comp: &PrimPath) -> anyhow::Result<(FrameRate, Time)> {
    let view = stage
        .prim(comp)
        .filter(|v| v.is_a(&PrimType::COMP))
        .ok_or_else(|| anyhow::anyhow!("{comp} is not a composition"))?;
    let rate = match sample(&view, "frame_rate", Time::ZERO) {
        Some(Value::FrameRate(r)) => r,
        _ => FrameRate::FPS_30,
    };
    let duration = sample(&view, "duration", Time::ZERO)
        .and_then(|v| v.as_time())
        .unwrap_or(Time::from_flicks(10 * Time::FLICKS_PER_SECOND));
    Ok((rate, duration))
}

/// Replaces the last run of `#` with the zero-padded frame number (or appends
/// `_NNNN` before the extension when there is none).
pub fn frame_file(pattern: &Path, frame: i64) -> PathBuf {
    let s = pattern.to_string_lossy();
    if let Some(end) = s.rfind('#') {
        let start = s[..=end].trim_end_matches('#').len();
        let width = end + 1 - start;
        return PathBuf::from(format!("{}{:0width$}{}", &s[..start], frame, &s[end + 1..]));
    }
    let stem = pattern
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = pattern
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_else(|| "png".into());
    pattern.with_file_name(format!("{stem}_{frame:04}.{ext}"))
}

fn run(m: &ArgMatches, desc: GpuContextDescriptor) -> anyhow::Result<ExitCode> {
    let project_path = m.get_one::<PathBuf>("project").expect("required");
    let comp = comp_path(m.get_one::<String>("comp").expect("required"))?;
    let json = m.get_flag("json");
    let project = Project::open(project_path)?;
    let stage = &project.stage;
    let (rate, duration) = comp_timing(stage, &comp)?;

    let frames: Vec<i64> = match (
        m.get_one::<String>("frames"),
        m.get_one::<i64>("frame"),
        m.get_one::<f64>("time"),
    ) {
        (Some(range), _, _) if range == "all" => {
            let last = rate.time_to_frame(duration).max(1);
            (0..last).collect()
        }
        (Some(range), _, _) => {
            let (a, b) = range
                .split_once('-')
                .ok_or_else(|| anyhow::anyhow!("bad --frames `{range}` (A-B or all)"))?;
            let (a, b): (i64, i64) = (a.trim().parse()?, b.trim().parse()?);
            anyhow::ensure!(a <= b, "bad --frames `{range}`");
            (a..=b).collect()
        }
        (None, Some(f), _) => vec![*f],
        (None, None, Some(_)) => vec![],
        (None, None, None) => vec![0],
    };
    let times: Vec<(Option<i64>, Time)> = match m.get_one::<f64>("time") {
        Some(t) if frames.is_empty() => vec![(None, Time::from_seconds(*t))],
        _ => frames
            .iter()
            .map(|f| (Some(*f), rate.frame_to_time(*f)))
            .collect(),
    };
    let sequence = times.len() > 1;
    let out_pattern = m.get_one::<PathBuf>("out").cloned().unwrap_or_else(|| {
        PathBuf::from(if sequence {
            format!("{}_####.png", comp.name())
        } else {
            format!("{}.png", comp.name())
        })
    });

    let request = RenderRequest {
        resolution: parse_resolution(m.get_one::<String>("resolution").expect("default"))?,
        roi: m
            .get_one::<String>("roi")
            .map(|s| parse_roi(s))
            .transpose()?,
        depth: match m.get_one::<String>("depth").map(String::as_str) {
            Some("16") => BitDepth::Bpc16,
            Some("32") => BitDepth::Float32,
            _ => BitDepth::Bpc8,
        },
        purpose: RenderPurpose::Final,
        ..RenderRequest::default()
    };
    let alpha_mode = m
        .get_one::<String>("alpha")
        .map(String::as_str)
        .unwrap_or("straight");

    let gpu = GpuContext::new_headless(desc)?;
    let mut renderer = Renderer::with_config(
        gpu.clone(),
        RendererConfig {
            // Every frame is new: no point caching for a batch render.
            frame_cache_bytes: 0,
            ..RendererConfig::default()
        },
    );
    let mut evaluator: Box<dyn CompEvaluator> = if m.get_flag("mock-eval") {
        Box::new(MockEvaluator::new(
            project.base_dir().map(Path::to_path_buf),
        ))
    } else {
        Box::new(AnimEvaluator::new(
            project.base_dir().map(Path::to_path_buf),
        ))
    };

    let started = Instant::now();
    let mut report = Vec::new();
    let mut all_warnings: Vec<String> = Vec::new();
    for (frame, time) in &times {
        let t0 = Instant::now();
        let rc = evaluator.evaluate(stage, &comp, *time)?;
        let eval_ms = t0.elapsed().as_secs_f64() * 1e3;
        let t1 = Instant::now();
        let rendered = renderer.render_frame(&rc, &request)?;
        let alpha = match alpha_mode {
            "premultiplied" => OutputAlpha::Premultiplied,
            "background" => {
                OutputAlpha::Over([rc.background[0], rc.background[1], rc.background[2]])
            }
            _ => OutputAlpha::Straight,
        };
        let file = if sequence {
            frame_file(&out_pattern, frame.unwrap_or_default())
        } else {
            out_pattern.clone()
        };
        let image = renderer.read_rgba8(&rendered, alpha)?;
        let render_ms = t1.elapsed().as_secs_f64() * 1e3;
        crate::png_out::write_rgba8(&file, &image)?;
        let mut warnings = evaluator.take_warnings();
        warnings.extend(rendered.warnings.iter().cloned());
        for w in &warnings {
            if !all_warnings.contains(w) {
                if !json {
                    eprintln!("warning: {w}");
                }
                all_warnings.push(w.clone());
            }
        }
        if !json {
            println!(
                "{}  t={:.3}s  {}×{}  eval {:.2} ms  render+readback {:.2} ms",
                file.display(),
                time.as_seconds(),
                image.width,
                image.height,
                eval_ms,
                render_ms
            );
        }
        report.push(serde_json::json!({
            "frame": frame,
            "time_flicks": time.flicks(),
            "file": file.to_string_lossy(),
            "width": image.width,
            "height": image.height,
            "eval_ms": eval_ms,
            "render_ms": render_ms,
            "draws": rendered.stats.draws,
            "draw_calls": rendered.stats.draw_calls,
        }));
    }
    let total_ms = started.elapsed().as_secs_f64() * 1e3;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "comp": comp.as_str(),
                "gpu": gpu.summary(),
                "frames": report,
                "total_ms": total_ms,
                "warnings": all_warnings,
            }))?
        );
    } else if sequence {
        println!(
            "{} frames in {:.1} ms ({:.1} fps) on {}",
            times.len(),
            total_ms,
            times.len() as f64 / (total_ms / 1e3),
            gpu.summary()
        );
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_saved_project_to_png() {
        let Ok(gpu) = GpuContext::new_headless(GpuContextDescriptor::default()) else {
            eprintln!("no GPU, skipping");
            return;
        };
        drop(gpu);
        let dir = tempfile::tempdir().unwrap();
        let mut b = crate::fixture::SceneBuilder::new();
        let comp = b
            .comp("main", 64, 32, FrameRate::FPS_30, Time::ONE_SECOND)
            .unwrap();
        b.solid_layer(&comp, "red", [1.0, 0.0, 0.0, 1.0], [32, 32], [0.0, 0.0])
            .unwrap();
        let project = dir.path().join("t.bae");
        Project::from_stage(b.into_stage())
            .save_as(&project)
            .unwrap();
        let out = dir.path().join("out_##.png");
        let cmd = args(clap::Command::new("render"));
        let m = cmd
            .try_get_matches_from([
                "render",
                project.to_str().unwrap(),
                "--comp",
                "main",
                "--frames",
                "0-1",
                "-o",
                out.to_str().unwrap(),
                "--json",
            ])
            .unwrap();
        assert_eq!(
            run(&m, GpuContextDescriptor::default()).unwrap(),
            ExitCode::SUCCESS
        );
        for f in ["out_00.png", "out_01.png"] {
            let (w, h, data) = crate::png_out::read_rgba8(&dir.path().join(f)).unwrap();
            assert_eq!((w, h), (64, 32));
            assert_eq!(&data[..4], &[255, 0, 0, 255]);
            assert_eq!(&data[(40 * 4)..(40 * 4 + 4)], &[0, 0, 0, 0]);
        }
    }

    #[test]
    fn frame_file_patterns() {
        assert_eq!(
            frame_file(Path::new("out/a_####.png"), 7),
            PathBuf::from("out/a_0007.png")
        );
        assert_eq!(
            frame_file(Path::new("a#.png"), 12),
            PathBuf::from("a12.png")
        );
        assert_eq!(
            frame_file(Path::new("out/a.png"), 3),
            PathBuf::from("out/a_0003.png")
        );
        assert_eq!(
            parse_roi("10,20,30,40").unwrap(),
            IRect::new(10, 20, 40, 60)
        );
        assert!(parse_roi("1,2,3").is_err());
        assert_eq!(comp_path("main").unwrap().as_str(), "/comps/main");
        assert_eq!(parse_resolution("5").unwrap(), Resolution::Custom(5));
    }
}
