//! [`RenderModule`]: the renderer's app module. Implements `bae render`:
//!
//! ```text
//! bae render <PROJECT> --comp <PATH|NAME> [--frame N | --time S | --frames A-B|all]
//!            [-o FILE|PATTERN] [--format png|tiff|exr|h264|hevc|prores|prores4444]
//!            [--compression LEVEL] [--bits 8|16|32] [--threads N]
//!            [--resolution full|half|third|quarter|N] [--depth 8|16|32]
//!            [--alpha straight|premultiplied|background] [--roi X,Y,W,H] [--json]
//! ```
//!
//! Without a window, on the shared GPU context; for CI, agents and render farms.
//! Sequences use the first run of `#` in the output file name for the frame
//! number (`out_####.png`, AE's `out_[####].png`).
//!
//! Export is pipelined ([`crate::export`]): the GPU renders frame N+1 while
//! frame N is read back asynchronously, and bae-media's sequence writer
//! compresses and writes files on every core behind a bounded queue. `--json`
//! reports where the time went (evaluation, rendering, readback, encoding,
//! writing) for comparisons (docs/perf.md).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use bae_app::{AppBuilder, AppModule};
use bae_gpu::{GpuContext, GpuContextDescriptor};
use bae_model::{FrameRate, PrimPath, PrimType, Project, Stage, Time, Value};
use bae_platform::cli::{self, CliSubcommand};
use clap::{Arg, ArgAction, ArgMatches};

use crate::anim::AnimEvaluator;
use crate::assets::AssetFiles;
use crate::eval::sample;
use crate::export::{
    Encoders, ExportAlpha, ExportFormat, ExportJob, ExportSettings, ExportedFrame, export_frames,
};
use crate::geom::IRect;
use crate::renderer::{Renderer, RendererConfig};
use crate::request::{BitDepth, RenderPurpose, RenderRequest, Resolution};
use crate::source::SourceProvider;

/// Registers `bae render`. Layer source providers come from the app's
/// `Registry<dyn SourceProvider>` (register with
/// `app.register_arc::<dyn bae_render::SourceProvider>(id, provider)`).
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
        let render = CliSubcommand::new(
            "render",
            "Render a composition without a window (PNG / TIFF / EXR sequences, movies)",
        )
        .with_args(args)
        .with_run(|ctx| {
            let desc = ctx
                .app
                .resource::<GpuContextDescriptor>()
                .cloned()
                .unwrap_or_default();
            #[cfg(feature = "media")]
            let media = ctx
                .app
                .resource::<std::sync::Arc<bae_media::MediaLibrary>>()
                .cloned();
            #[cfg(not(feature = "media"))]
            let media: Option<MediaHandle> = None;
            // Encoders registered by bae-media's module and plugins; the
            // built-in ones when none are.
            #[cfg(feature = "media")]
            let encoders = {
                let registry = ctx.app.registry::<dyn bae_media::EncoderFormat>();
                Some(bae_media::MediaEncoders::from_registry(&registry))
                    .filter(|e| e.iter().next().is_some())
            };
            #[cfg(not(feature = "media"))]
            let encoders: Option<Encoders> = None;
            // Layer source providers registered by other modules (05's shapes
            // and text, plugin generators).
            let providers: Vec<std::sync::Arc<dyn SourceProvider>> = ctx
                .app
                .registry::<dyn SourceProvider>()
                .snapshot()
                .iter()
                .map(|e| e.item().clone())
                .collect();
            // The app's wording for projects that do not open (same as
            // `bae export-json`).
            let open_error = ctx.app.resource::<cli::ProjectOpenError>().cloned();
            run(
                ctx.matches,
                desc,
                media,
                encoders,
                &providers,
                open_error.as_ref(),
            )
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
            .help(
                "Output file; for sequences a pattern with #### \
                 (default: <comp>[_####].<format extension>)",
            ),
    )
    .arg(
        Arg::new("format")
            .long("format")
            .short('f')
            .value_name("FORMAT")
            .value_parser(ExportFormat::NAMES)
            .help(
                "png | tiff | exr image sequences, or h264 | hevc | prores (422 HQ) | \
                 prores4444 movies (macOS). Default: from the -o extension, else png",
            ),
    )
    .arg(
        Arg::new("compression")
            .long("compression")
            .value_name("LEVEL")
            .help(
                "png: none | fastest | fast (default) | balanced | high | 0-9; \
                 tiff: none (default) | lzw | deflate; exr: none | rle | zips | zip (default) | piz",
            ),
    )
    .arg(
        Arg::new("bits")
            .long("bits")
            .value_name("BPC")
            .value_parser(["8", "16", "32"])
            .help("Output bits per channel: png / tiff 8 (default) or 16; exr 16 (half) or 32"),
    )
    .arg(
        Arg::new("threads")
            .long("threads")
            .value_name("N")
            .value_parser(clap::value_parser!(u32).range(1..))
            .help("Encoder threads for image sequences (default: one per core)"),
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
            .help(
                "RGB + alpha, straight or premultiplied (EXR: always premultiplied); or RGB \
                 over the comp background (always for codecs without alpha)",
            ),
    )
    .arg(
        Arg::new("roi")
            .long("roi")
            .value_name("X,Y,W,H")
            .help("Render only this region (comp pixels)"),
    )
    .arg(
        Arg::new("json")
            .long("json")
            .action(ArgAction::SetTrue)
            .help("Print a machine-readable JSON report (with per-stage timings)"),
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

/// The app's shared media library, when footage decoding is compiled in.
#[cfg(feature = "media")]
type MediaHandle = std::sync::Arc<bae_media::MediaLibrary>;
#[cfg(not(feature = "media"))]
type MediaHandle = ();

fn run(
    m: &ArgMatches,
    desc: GpuContextDescriptor,
    media: Option<MediaHandle>,
    encoders: Option<Encoders>,
    providers: &[std::sync::Arc<dyn SourceProvider>],
    open_error: Option<&cli::ProjectOpenError>,
) -> anyhow::Result<ExitCode> {
    let entered = Instant::now();
    let project_path = m.get_one::<PathBuf>("project").expect("required");
    let comp = comp_path(m.get_one::<String>("comp").expect("required"))?;
    let json = m.get_flag("json");
    let project = Project::open(project_path)
        .map_err(|error| cli::project_open_error(open_error, project_path, error.into()))?;
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
    let format = match m.get_one::<String>("format") {
        Some(name) => ExportFormat::parse(name).expect("checked by clap"),
        None => m
            .get_one::<PathBuf>("out")
            .and_then(|p| ExportFormat::from_path(p))
            .unwrap_or_default(),
    };
    let out_pattern = m.get_one::<PathBuf>("out").cloned().unwrap_or_else(|| {
        PathBuf::from(if sequence && format.is_image() {
            format!("{}_####.{}", comp.name(), format.extension())
        } else {
            format!("{}.{}", comp.name(), format.extension())
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
    let settings = ExportSettings {
        compression: m.get_one::<String>("compression").cloned(),
        bits: m
            .get_one::<String>("bits")
            .map(|b| b.parse().expect("checked by clap")),
        threads: m.get_one::<u32>("threads").map(|n| *n as usize),
        alpha: match m.get_one::<String>("alpha").map(String::as_str) {
            Some("premultiplied") => ExportAlpha::Premultiplied,
            Some("background") => ExportAlpha::Background,
            _ => ExportAlpha::Straight,
        },
        ..ExportSettings::new(format, &out_pattern)
    };
    // Reject bad output options before any GPU work.
    settings.validate(rate, &request)?;

    let gpu = GpuContext::new_headless(desc)?;
    let mut renderer = Renderer::with_config(
        gpu.clone(),
        RendererConfig {
            // Every frame is new: no point caching for a batch render.
            frame_cache_bytes: 0,
            ..RendererConfig::default()
        },
    );
    #[cfg(feature = "media")]
    if let Some(library) = media {
        renderer.set_media_library(library);
    }
    #[cfg(not(feature = "media"))]
    let _ = media;
    for provider in providers {
        renderer.providers_mut().register(provider.clone());
    }
    // Footage on disk or inside the .baez the project was opened from.
    let assets = AssetFiles::for_project(&project)?;
    let mut evaluator = AnimEvaluator::new(None).with_assets(assets);

    let setup = entered.elapsed();
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    let mut print_frame = |f: &ExportedFrame| {
        if !json {
            println!(
                "{}  t={:.3}s  {}×{}  eval {:.2} ms  render {:.2} ms",
                f.file.display(),
                f.time.as_seconds(),
                f.width,
                f.height,
                ms(f.eval),
                ms(f.render)
            );
        }
    };
    let report = export_frames(
        &mut renderer,
        &mut evaluator,
        &ExportJob {
            stage,
            comp: &comp,
            times: &times,
            rate,
            request: &request,
            settings: &settings,
            encoders: encoders.as_ref(),
        },
        None,
        &mut print_frame,
    )?;
    let s = report.stats;
    let fps = s.frames as f64 / s.wall.as_secs_f64().max(1e-9);
    let compression = settings
        .compression
        .clone()
        .unwrap_or_else(|| format.default_compression().to_owned());
    if json {
        let frames: Vec<serde_json::Value> = report
            .frames
            .iter()
            .map(|f| {
                serde_json::json!({
                    "frame": f.frame,
                    "time_flicks": f.time.flicks(),
                    "file": f.file.to_string_lossy(),
                    "width": f.width,
                    "height": f.height,
                    "eval_ms": ms(f.eval),
                    "render_ms": ms(f.render),
                    "draws": f.draws,
                    "draw_calls": f.draw_calls,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "comp": comp.as_str(),
                "gpu": gpu.summary(),
                "output": {
                    "format": format.name(),
                    "compression": compression,
                    "pattern": out_pattern.to_string_lossy(),
                    "files": s.files,
                    "bytes": s.bytes,
                    "encoder_threads": s.encoder_threads,
                },
                "frames": frames,
                "stages": {
                    "setup_ms": ms(setup),
                    "eval_ms": ms(s.eval),
                    "render_ms": ms(s.render),
                    "readback_wait_ms": ms(s.readback_wait),
                    "readback_copy_ms": ms(s.readback_copy),
                    "encoder_stall_ms": ms(s.encoder_stall),
                    "encode_ms": ms(s.encode),
                    "write_ms": ms(s.write),
                    "drain_ms": ms(s.drain),
                },
                "throughput": {
                    "fps": fps,
                    "mb_per_s": s.bytes as f64 / 1e6 / s.wall.as_secs_f64().max(1e-9),
                },
                "total_ms": ms(s.wall),
                "warnings": report.warnings,
            }))?
        );
    } else {
        for w in &report.warnings {
            eprintln!("warning: {w}");
        }
        if sequence {
            println!(
                "{} frames in {:.1} ms ({:.1} fps) on {} — {} ({}), {} encoder thread(s), {:.1} MB",
                s.frames,
                ms(s.wall),
                fps,
                gpu.summary(),
                format.name(),
                compression,
                s.encoder_threads,
                s.bytes as f64 / 1e6
            );
            println!(
                "  setup {:.1} ms · eval {:.1} · render {:.1} · readback wait {:.1} · copy {:.1} · \
                 encoder stall {:.1} · drain {:.1} ms; encode {:.1} · write {:.1} ms (summed over \
                 threads)",
                ms(setup),
                ms(s.eval),
                ms(s.render),
                ms(s.readback_wait),
                ms(s.readback_copy),
                ms(s.encoder_stall),
                ms(s.drain),
                ms(s.encode),
                ms(s.write),
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::CompEvaluator;
    use crate::export::frame_file;
    use bae_model::Value;
    use std::path::Path;

    /// A project opened from a `.baez` finds its embedded footage after the
    /// original file is gone (extracted through bae-package's locator).
    #[cfg(feature = "media")]
    #[test]
    fn renders_packaged_footage_from_baez() {
        if GpuContext::new_headless(GpuContextDescriptor::default()).is_err() {
            eprintln!("no GPU, skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("src/red.png");
        crate::fixture::write_label_png(&png, 16, 8, 0.0).unwrap();
        let mut b = crate::fixture::SceneBuilder::new();
        let comp = b
            .comp("main", 32, 16, FrameRate::FPS_30, Time::ONE_SECOND)
            .unwrap();
        let project_dir = dir.path().join("p/T.bae");
        let asset = b.footage_asset("red", &png, None, Some([16, 8])).unwrap();
        let layer = b.layer(&comp, "img", "footage", Some(&asset)).unwrap();
        b.set(
            &layer,
            "transform:anchor_point",
            Value::Vec3([8.0, 4.0, 0.0]),
        )
        .unwrap();
        b.set(&layer, "transform:position", Value::Vec3([16.0, 8.0, 0.0]))
            .unwrap();
        let mut project = Project::from_stage(b.into_stage());
        project.save_as(&project_dir).unwrap();
        let baez = dir.path().join("handoff/T.baez");
        std::fs::create_dir_all(baez.parent().unwrap()).unwrap();
        bae_package::pack(
            &bae_package::PackSource::project(&project),
            &baez,
            &bae_package::PackOptions::default(),
            bae_package::WriteOptions::default(),
        )
        .unwrap();
        // The original footage and project are gone; only the .baez remains.
        std::fs::remove_dir_all(dir.path().join("src")).unwrap();
        std::fs::remove_dir_all(dir.path().join("p")).unwrap();

        let out = dir.path().join("out.png");
        let m = args(clap::Command::new("render"))
            .try_get_matches_from([
                "render",
                baez.to_str().unwrap(),
                "--comp",
                "main",
                "-o",
                out.to_str().unwrap(),
                "--alpha",
                "premultiplied",
            ])
            .unwrap();
        assert_eq!(
            run(&m, GpuContextDescriptor::default(), None, None, &[], None).unwrap(),
            ExitCode::SUCCESS
        );
        let (w, _, data) = crate::png_out::read_rgba8(&out).unwrap();
        // Centre of the label is opaque; outside the layer is transparent.
        let px = |x: u32, y: u32| &data[((y * w + x) * 4) as usize..((y * w + x) * 4 + 4) as usize];
        assert_eq!(px(16, 8)[3], 255, "{:?}", px(16, 8));
        assert_eq!(px(2, 2), &[0, 0, 0, 0]);
    }

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
            run(&m, GpuContextDescriptor::default(), None, None, &[], None).unwrap(),
            ExitCode::SUCCESS
        );
        for f in ["out_00.png", "out_01.png"] {
            let (w, h, data) = crate::png_out::read_rgba8(&dir.path().join(f)).unwrap();
            assert_eq!((w, h), (64, 32));
            assert_eq!(&data[..4], &[255, 0, 0, 255]);
            assert_eq!(&data[(40 * 4)..(40 * 4 + 4)], &[0, 0, 0, 0]);
        }
    }

    /// A small animated scene: rows not 256-byte aligned (staging buffer
    /// padding), transparent areas (alpha modes), a backdrop-reading blend mode
    /// and motion (every frame differs). Returns the saved project.
    fn animated_project(dir: &Path) -> PathBuf {
        let mut b = crate::fixture::SceneBuilder::new();
        let comp = b
            .comp("main", 37, 23, FrameRate::FPS_30, Time::ONE_SECOND)
            .unwrap();
        let a = b
            .solid_layer(&comp, "a", [1.0, 0.5, 0.25, 1.0], [12, 9], [0.0, 2.0])
            .unwrap();
        b.key(
            &a,
            "transform:position",
            Time::ZERO,
            Value::Vec3([6.0, 6.5, 0.0]),
        )
        .unwrap();
        b.key(
            &a,
            "transform:position",
            Time::ONE_SECOND,
            Value::Vec3([31.0, 16.5, 0.0]),
        )
        .unwrap();
        b.set(&a, "transform:opacity", Value::Float(60.0)).unwrap();
        let m = b
            .solid_layer(&comp, "m", [0.2, 0.9, 0.6, 1.0], [20, 14], [8.0, 4.0])
            .unwrap();
        b.set(&m, "blend_mode", Value::Enum("multiply".into()))
            .unwrap();
        b.key(&m, "transform:rotation", Time::ZERO, Value::Float(0.0))
            .unwrap();
        b.key(
            &m,
            "transform:rotation",
            Time::ONE_SECOND,
            Value::Float(90.0),
        )
        .unwrap();
        b.solid_layer(&comp, "under", [0.1, 0.2, 0.7, 1.0], [18, 23], [19.0, 0.0])
            .unwrap();
        let project = dir.join("anim.bae");
        Project::from_stage(b.into_stage())
            .save_as(&project)
            .unwrap();
        project
    }

    /// The pre-pipeline export: evaluate, render, blocking 8-bit readback, one
    /// frame at a time.
    fn serial_frames(
        project: &Path,
        frames: std::ops::RangeInclusive<i64>,
        alpha: ExportAlpha,
    ) -> Vec<Vec<u8>> {
        let project = Project::open(project).unwrap();
        let gpu = GpuContext::new_headless(GpuContextDescriptor::default()).unwrap();
        let mut renderer = Renderer::with_config(
            gpu,
            RendererConfig {
                frame_cache_bytes: 0,
                ..RendererConfig::default()
            },
        );
        let mut eval =
            AnimEvaluator::new(None).with_assets(AssetFiles::for_project(&project).unwrap());
        let request = RenderRequest {
            purpose: RenderPurpose::Final,
            ..RenderRequest::default()
        };
        let comp = comp_path("main").unwrap();
        frames
            .map(|f| {
                let rc = eval
                    .evaluate(&project.stage, &comp, FrameRate::FPS_30.frame_to_time(f))
                    .unwrap();
                let frame = renderer.render_frame(&rc, &request).unwrap();
                renderer
                    .read_rgba8(&frame, alpha.output(rc.background))
                    .unwrap()
                    .data
            })
            .collect()
    }

    fn render_cli(argv: &[&str]) -> anyhow::Result<ExitCode> {
        let m = args(clap::Command::new("render"))
            .try_get_matches_from(std::iter::once("render").chain(argv.iter().copied()))
            .unwrap();
        run(&m, GpuContextDescriptor::default(), None, None, &[], None)
    }

    fn has_gpu() -> bool {
        let ok = GpuContext::new_headless(GpuContextDescriptor::default()).is_ok();
        if !ok {
            eprintln!("no GPU, skipping");
        }
        ok
    }

    /// The pipelined export (async readback ring, encoder threads) writes
    /// exactly the pixels of the old serial path, for every alpha mode, any
    /// thread count and PNG compression level, each frame in its own file.
    #[test]
    fn pipelined_export_matches_serial_render() {
        if !has_gpu() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let project = animated_project(dir.path());
        let mut cases: Vec<(&str, ExportAlpha, &[&str])> = vec![
            ("straight", ExportAlpha::Straight, &[]),
            ("premultiplied", ExportAlpha::Premultiplied, &[]),
            ("background", ExportAlpha::Background, &[]),
        ];
        // Encoder threads and compression levels (bae-media).
        if cfg!(feature = "media") {
            cases.extend([
                (
                    "premultiplied",
                    ExportAlpha::Premultiplied,
                    &["--threads", "3"][..],
                ),
                (
                    "background",
                    ExportAlpha::Background,
                    &["--compression", "fastest"][..],
                ),
                (
                    "straight",
                    ExportAlpha::Straight,
                    &["--threads", "1", "--compression", "high"][..],
                ),
            ]);
        }
        for (case, (alpha_arg, alpha, extra)) in cases.iter().enumerate() {
            let out = dir.path().join(format!("case{case}/f_####.png"));
            let mut argv = vec![
                project.to_str().unwrap(),
                "--comp",
                "main",
                "--frames",
                "2-13",
                "-o",
                out.to_str().unwrap(),
                "--alpha",
                alpha_arg,
                "--json",
            ];
            argv.extend_from_slice(extra);
            assert_eq!(render_cli(&argv).unwrap(), ExitCode::SUCCESS);
            let expected = serial_frames(&project, 2..=13, *alpha);
            for (i, want) in expected.iter().enumerate() {
                let file = frame_file(&out, 2 + i as i64);
                let (w, h, got) = crate::png_out::read_rgba8(&file).unwrap();
                assert_eq!((w, h), (37, 23));
                assert!(
                    got == *want,
                    "case {case} ({alpha_arg} {extra:?}): {} differs from the serial render",
                    file.display()
                );
            }
        }
        // Frames really differ (so a mix-up of files would be caught).
        let frames = serial_frames(&project, 2..=3, ExportAlpha::Straight);
        assert_ne!(frames[0], frames[1]);
    }

    /// Files are numbered by comp frame, contiguous, with nothing extra; the
    /// naming matches `frame_file` (and bae-media's pattern parser).
    #[test]
    fn sequence_files_are_contiguous_and_named() {
        if !has_gpu() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let project = animated_project(dir.path());
        let out = dir.path().join("seq/f_[###].png");
        assert_eq!(
            render_cli(&[
                project.to_str().unwrap(),
                "--comp",
                "main",
                "--frames",
                "7-18",
                "-o",
                out.to_str().unwrap(),
                "--json",
            ])
            .unwrap(),
            ExitCode::SUCCESS
        );
        let mut names: Vec<String> = std::fs::read_dir(dir.path().join("seq"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let expected: Vec<String> = (7..=18).map(|f| format!("f_{f:03}.png")).collect();
        assert_eq!(names, expected);
        // A single frame goes to exactly the given path.
        let single = dir.path().join("one/still.png");
        assert_eq!(
            render_cli(&[
                project.to_str().unwrap(),
                "--comp",
                "main",
                "--frame",
                "4",
                "-o",
                single.to_str().unwrap(),
                "--json",
            ])
            .unwrap(),
            ExitCode::SUCCESS
        );
        let (_, _, still) = crate::png_out::read_rgba8(&single).unwrap();
        assert_eq!(
            still,
            serial_frames(&project, 4..=4, ExportAlpha::Straight)[0]
        );
    }

    /// A file that cannot be written stops the export with that error (naming
    /// the file), the encoder threads exit, and the frames written before it
    /// are kept.
    #[test]
    fn write_failure_stops_cleanly() {
        if !has_gpu() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let project = animated_project(dir.path());
        let out_dir = dir.path().join("out");
        // A directory where frame 5's file should go.
        std::fs::create_dir_all(out_dir.join("f_0005.png")).unwrap();
        let error = render_cli(&[
            project.to_str().unwrap(),
            "--comp",
            "main",
            "--frames",
            "0-29",
            "-o",
            out_dir.join("f_####.png").to_str().unwrap(),
            "--json",
        ])
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("f_0005.png"),
            "error names the file: {error:#}"
        );
        for f in 0..5 {
            let file = frame_file(&out_dir.join("f_####.png"), f);
            let (w, _, _) = crate::png_out::read_rgba8(&file)
                .unwrap_or_else(|e| panic!("{} kept: {e}", file.display()));
            assert_eq!(w, 37);
        }

        // An output directory that cannot be created fails up front.
        let blocker = dir.path().join("not_a_dir");
        std::fs::write(&blocker, b"x").unwrap();
        assert!(
            render_cli(&[
                project.to_str().unwrap(),
                "--comp",
                "main",
                "--frames",
                "0-3",
                "-o",
                blocker.join("f_####.png").to_str().unwrap(),
            ])
            .is_err()
        );
        // Bad options are rejected before rendering.
        #[cfg(feature = "media")]
        assert!(
            render_cli(&[
                project.to_str().unwrap(),
                "--comp",
                "main",
                "--format",
                "tiff",
                "--compression",
                "fastest",
            ])
            .is_err()
        );
    }

    /// TIFF (uncompressed, AE's comparison format) and EXR sequences decode to
    /// the serial render.
    #[cfg(feature = "media")]
    #[test]
    fn tiff_and_exr_sequences() {
        use bae_media::{AlphaMode, FrameRequest, MediaFormats, OpenOptions, PixelFormat};
        if !has_gpu() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let project = animated_project(dir.path());
        let expected = serial_frames(&project, 0..=5, ExportAlpha::Straight);
        let formats = MediaFormats::builtin();
        for (format, ext, extra, tolerance) in [
            ("tiff", "tif", &[][..], 0u8),
            (
                "tiff",
                "tif",
                &["--compression", "lzw", "--bits", "16"][..],
                1,
            ),
            ("exr", "exr", &["--compression", "none"][..], 1),
            ("exr", "exr", &["--bits", "32"][..], 1),
        ] {
            let out = dir
                .path()
                .join(format!("{format}{}/f_####.{ext}", extra.len()));
            let mut argv = vec![
                project.to_str().unwrap(),
                "--comp",
                "main",
                "--frames",
                "0-5",
                "-o",
                out.to_str().unwrap(),
                "--format",
                format,
                "--json",
            ];
            argv.extend_from_slice(extra);
            assert_eq!(render_cli(&argv).unwrap(), ExitCode::SUCCESS);
            for (f, want) in expected.iter().enumerate() {
                let file = frame_file(&out, f as i64);
                let mut source = formats
                    .open(&file, &OpenOptions::default())
                    .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
                let frame = source
                    .decode_cpu(&FrameRequest {
                        alpha_output: bae_media::AlphaOutput::Straight,
                        ..FrameRequest::new(0)
                    })
                    .unwrap();
                let frame = bae_media::export::convert_frame(
                    &frame,
                    PixelFormat::Rgba8,
                    AlphaMode::Straight,
                );
                let worst = frame
                    .data
                    .iter()
                    .zip(want)
                    .enumerate()
                    // Colour of fully transparent pixels is undefined.
                    .filter(|(i, _)| want[i / 4 * 4 + 3] != 0)
                    .map(|(_, (a, b))| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0);
                assert!(
                    worst <= tolerance,
                    "{format} {extra:?} {}: off by {worst}",
                    file.display()
                );
            }
        }
    }

    /// The library entry point (what the render queue runs on a worker
    /// thread): progress per frame in order; stopping keeps what was rendered.
    #[test]
    fn export_frames_reports_progress_and_cancels() {
        if !has_gpu() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let project = Project::open(animated_project(dir.path())).unwrap();
        let gpu = GpuContext::new_headless(GpuContextDescriptor::default()).unwrap();
        let mut renderer = Renderer::new(gpu);
        let mut evaluator = AnimEvaluator::new(None);
        let comp = comp_path("main").unwrap();
        let times: Vec<(Option<i64>, Time)> = (0..20)
            .map(|f| (Some(f), FrameRate::FPS_30.frame_to_time(f)))
            .collect();
        let request = RenderRequest {
            purpose: RenderPurpose::Final,
            ..RenderRequest::default()
        };
        // TIFF (uncompressed, so the byte count is known) where bae-media is in.
        let (format, ext) = if cfg!(feature = "media") {
            (ExportFormat::Tiff, "tif")
        } else {
            (ExportFormat::Png, "png")
        };
        let settings = ExportSettings::new(format, dir.path().join(format!("q/f_####.{ext}")));
        let job = ExportJob {
            stage: &project.stage,
            comp: &comp,
            times: &times,
            rate: FrameRate::FPS_30,
            request: &request,
            settings: &settings,
            encoders: None,
        };
        let mut seen = Vec::new();
        let report = export_frames(&mut renderer, &mut evaluator, &job, None, &mut |f| {
            seen.push(f.frame.unwrap())
        })
        .unwrap();
        assert_eq!(seen, (0..20).collect::<Vec<_>>());
        assert_eq!((report.stats.frames, report.stats.files), (20, 20));
        if format == ExportFormat::Tiff {
            assert!(report.stats.bytes >= 20 * 37 * 23 * 4);
        }
        assert_eq!(
            report.frames[7].file,
            dir.path().join(format!("q/f_0007.{ext}"))
        );

        let cancel = std::sync::atomic::AtomicBool::new(false);
        let settings = ExportSettings::new(ExportFormat::Png, dir.path().join("c/f_####.png"));
        let job = ExportJob {
            settings: &settings,
            ..job
        };
        let report = export_frames(
            &mut renderer,
            &mut evaluator,
            &job,
            Some(&cancel),
            &mut |f| {
                if f.index == 3 {
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            },
        )
        .unwrap();
        // Stopped: the frames rendered before Stop are all written and kept (AE).
        assert!(report.stopped);
        let n = report.frames.len();
        assert!((4..20).contains(&n), "{n}");
        assert_eq!(report.stats.files, n);
        let left = std::fs::read_dir(dir.path().join("c")).unwrap().count();
        assert_eq!(left, n, "stopped output kept");
        for (i, f) in report.frames.iter().enumerate() {
            assert_eq!(f.frame, Some(i as i64), "contiguous from the start");
            assert!(f.file.is_file());
        }
    }

    #[cfg(feature = "media")]
    #[test]
    fn frame_file_matches_media_sequence_naming() {
        for pattern in [
            "out/a_####.png",
            "a#.png",
            "out/a.png",
            "x/s_[###].exr",
            "b.0000.tif",
        ] {
            let pattern = Path::new(pattern);
            let media = bae_media::export::SequencePattern::parse(
                &crate::export::explicit_pattern(pattern),
            );
            for frame in [-3, 0, 7, 12345] {
                assert_eq!(
                    frame_file(pattern, frame),
                    media.path_for(frame),
                    "{pattern:?}"
                );
            }
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
            frame_file(Path::new("out/a_[####].png"), 3),
            PathBuf::from("out/a_0003.png")
        );
        assert_eq!(
            frame_file(Path::new("a_##.png"), -3),
            PathBuf::from("a_-03.png")
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("x/a_####.TIF")),
            Some(ExportFormat::Tiff)
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("a.mov")),
            Some(ExportFormat::ProRes)
        );
        assert_eq!(ExportFormat::from_path(Path::new("a.jpg")), None);
        assert_eq!(
            parse_roi("10,20,30,40").unwrap(),
            IRect::new(10, 20, 40, 60)
        );
        assert!(parse_roi("1,2,3").is_err());
        assert_eq!(comp_path("main").unwrap().as_str(), "/comps/main");
        assert_eq!(parse_resolution("5").unwrap(), Resolution::Custom(5));
    }
}
