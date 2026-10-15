//! Gremliner IDE entry point.
//!
//! Normal runs open the workspace shell. `--graph-demo` and `--render-bench` open the bare graph canvas
//! used by the rendering gate (ADR-0006, ADR-0032).

// Release builds on Windows are GUI-subsystem programs, so launching from the Start menu doesn't also open a
// console window (ADR-0040). Their stdout goes nowhere; `gremliner-cli` is the terminal tool there.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod gpu;
mod measure;

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use gpui_kit::*;
use graph_canvas::GraphCanvas;
use graph_canvas::bench::BenchRun;
use storage::Storage;
use ui_workspace::assets::AppAssets;
use ui_workspace::persist::{self, WindowGeometry};
use ui_workspace::theme::ThemeChoice;
use ui_workspace::{ShellConfig, Workspace, WorkspaceOptions};

#[derive(Parser, Debug, Clone)]
#[command(name = "gremliner", version, about = "Gremliner IDE")]
struct Args {
    /// Show the frame-time overlay.
    #[arg(long)]
    perf_hud: bool,
    /// Quit automatically after this many seconds.
    #[arg(long, value_name = "SECONDS")]
    exit_after: Option<f32>,
    /// Theme for this run, without changing the saved setting.
    #[arg(long, value_enum)]
    theme: Option<ThemeArg>,
    /// Directory for the app database (default: the platform data directory).
    #[arg(long, value_name = "DIR", env = "GREMLINER_DATA_DIR")]
    data_dir: Option<PathBuf>,
    /// Keep all state in memory for this run.
    #[arg(long)]
    no_persist: bool,
    /// Connect to a saved connection by name on startup.
    #[arg(long, value_name = "NAME")]
    open_connection: Option<String>,
    /// Dispatch an action after the window opens, e.g. `workspace::OpenCommandPalette` (repeatable;
    /// used by screenshots and agents to reach a UI state).
    #[arg(long = "command", value_name = "ACTION")]
    commands: Vec<String>,
    /// Wait this many seconds after the first frame before `--ask` and `--command`s (for states that
    /// need a connection, a loaded schema or query results first; the connection opens right away).
    #[arg(long, value_name = "SECONDS", default_value_t = 0.0)]
    command_delay: f32,
    /// Open a tab with this query and run it on startup (on `--open-connection`'s connection).
    #[arg(long, value_name = "GREMLIN")]
    run: Option<String>,
    /// Open a tab and ask the assistant this on startup (on `--open-connection`'s connection).
    #[arg(long, value_name = "PROMPT", conflicts_with = "run")]
    ask: Option<String>,
    /// Don't offer the first-run tour (scripted screenshots of other surfaces).
    #[arg(long)]
    no_tour: bool,
    /// Print the time from launch to the first frame (`measure first_frame_ms=..`), then quit.
    #[arg(long)]
    measure_startup: bool,
    /// Stay idle this many seconds, print resident memory (`measure idle_rss=.. MB`), then quit.
    #[arg(long, value_name = "SECONDS")]
    measure_idle_memory: Option<f32>,
    /// Run the scripted render benchmark on a synthetic graph: NODES,EDGES (e.g. 10000,20000), then exit.
    #[arg(long, value_name = "NODES,EDGES")]
    render_bench: Option<String>,
    /// Benchmark duration in seconds.
    #[arg(long, default_value_t = 10.0)]
    bench_seconds: f32,
    /// Open an interactive canvas on a synthetic graph: NODES,EDGES.
    #[arg(long, value_name = "NODES,EDGES")]
    graph_demo: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ThemeArg {
    Light,
    Dark,
    System,
}

impl From<ThemeArg> for ThemeChoice {
    fn from(arg: ThemeArg) -> Self {
        match arg {
            ThemeArg::Light => ThemeChoice::Light,
            ThemeArg::Dark => ThemeChoice::Dark,
            ThemeArg::System => ThemeChoice::System,
        }
    }
}

fn parse_pair(s: &str) -> Option<(usize, usize)> {
    let (a, b) = s.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// Window geometry in logical pixels (not design tokens).
const DEFAULT_WINDOW: [f32; 2] = [1440.0, 900.0];
const MIN_WINDOW: [f32; 2] = [720.0, 480.0];
const BENCH_WINDOW: [f32; 2] = [1600.0, 1000.0];

fn open_storage(args: &Args) -> Option<Storage> {
    if args.no_persist {
        return None;
    }
    let dir = args.data_dir.clone().or_else(Storage::default_data_dir)?;
    match Storage::open_in_dir(&dir) {
        Ok(storage) => Some(storage),
        Err(err) => {
            log::error!(
                "could not open the app database in {}: {err}; settings will not be saved",
                dir.display()
            );
            None
        }
    }
}

/// env_logger behind the secret redactor (ADR-0034): registered secrets never reach the log.
fn init_logging() {
    let logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).build();
    let max_level = logger.filter();
    if log::set_boxed_logger(Box::new(storage::secrets::RedactingLogger::new(logger))).is_ok() {
        log::set_max_level(max_level);
    }
}

fn main() {
    measure::mark_start();
    init_logging();
    // Before any thread starts (it sets environment variables for the GPU loaders).
    gpu::select_drivers();
    let args = Args::parse();
    let graph = args
        .render_bench
        .as_deref()
        .or(args.graph_demo.as_deref())
        .map(|s| parse_pair(s).unwrap_or_else(|| panic!("expected NODES,EDGES, got {s:?}")));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_name("gremliner-io")
        .enable_all()
        .build()
        .expect("failed to start the I/O runtime");
    // The runtime lives as long as the process; worker threads end with it.
    let runtime: &'static tokio::runtime::Runtime = Box::leak(Box::new(runtime));
    measure::checkpoint("runtime started");
    let storage = if graph.is_some() {
        None
    } else {
        open_storage(&args)
    };
    measure::checkpoint("storage opened");
    let geometry = storage.as_ref().and_then(persist::load_window);

    gpui_kit::application()
        .with_assets(AppAssets)
        .run(move |cx| {
            measure::checkpoint("platform ready");
            gpui_kit::init(cx);
            measure::checkpoint("gpui-kit initialised");
            ui_workspace::io::install(runtime.handle().clone(), cx);

            if let Some((nodes, edges)) = graph {
                open_graph_canvas(&args, nodes, edges, cx);
            } else {
                ui_workspace::init(
                    ShellConfig {
                        storage,
                        theme_override: args.theme.map(Into::into),
                        secrets: None,
                    },
                    cx,
                );
                measure::checkpoint("shell initialised");
                ui_workspace::motion::watch_os_preference(cx);
                open_workspace(&args, geometry, cx);
                measure::checkpoint("window opened");
            }
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            if let Some(secs) = args.measure_idle_memory {
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(Duration::from_secs_f32(secs))
                        .await;
                    match measure::resident_mb() {
                        Some(mb) => measure::report("idle_rss", mb, "MB"),
                        None => log::error!("resident memory is not available on this platform"),
                    }
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
            if let Some(secs) = args.exit_after {
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(Duration::from_secs_f32(secs))
                        .await;
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
        });
}

fn open_workspace(args: &Args, geometry: Option<WindowGeometry>, cx: &mut App) {
    let min = size(px(MIN_WINDOW[0]), px(MIN_WINDOW[1]));
    let bounds = geometry.and_then(|g| g.to_bounds(min)).unwrap_or_else(|| {
        WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(DEFAULT_WINDOW[0]), px(DEFAULT_WINDOW[1])),
            cx,
        ))
    });
    let options = WindowOptions {
        window_bounds: Some(bounds),
        window_min_size: Some(min),
        titlebar: Some(TitlebarOptions {
            title: Some("Gremliner".into()),
            ..Default::default()
        }),
        app_id: Some("gremliner".into()),
        ..Default::default()
    };
    let workspace_options = WorkspaceOptions {
        perf_hud: args.perf_hud,
        // Benchmarks and scripted screenshots start from a clean slate without the tour card.
        tour: !args.no_tour,
    };
    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        measure::checkpoint("renderer ready");
        let workspace = cx.new(|cx| Workspace::new(workspace_options, window, cx));
        measure::checkpoint("workspace built");
        window.focus(&workspace.read(cx).focus_handle().clone(), cx);
        workspace
    });
    match opened {
        Ok((handle, workspace)) => {
            let commands = args.commands.clone();
            let mut connection = None;
            if let Some(name) = &args.open_connection {
                let manager = ui_workspace::connections::ConnectionManager::global(cx);
                connection = manager.read(cx).find_by_name(name).map(|c| c.id);
                match connection {
                    Some(id)
                        if args.run.is_none()
                            && (args.ask.is_none() || args.command_delay > 0.0) =>
                    {
                        manager.update(cx, |m, cx| m.connect(id, cx)).detach()
                    }
                    Some(_) => {}
                    None => log::error!("--open-connection: no saved connection named {name:?}"),
                }
            }
            let run = args.run.clone();
            let ask = args.ask.clone();
            let workspace = workspace.clone();
            let measure_startup = args.measure_startup;
            let command_delay = args.command_delay;
            let _ = handle.update(cx, |_, window, _| {
                // Actions route through the rendered element tree, so they wait for the first frame.
                window.on_next_frame(move |window, cx| {
                    log::info!("first frame rendered");
                    if measure_startup {
                        measure::report("first_frame_ms", measure::since_start_ms(), "ms");
                        cx.quit();
                        return;
                    }
                    if let Some(query) = &run {
                        workspace
                            .update(cx, |ws, cx| ws.open_and_run(query, connection, window, cx));
                    }
                    let workspace = workspace.clone();
                    let commands = commands.clone();
                    let ask = ask.clone();
                    let later = move |window: &mut Window, cx: &mut App| {
                        if let Some(prompt) = &ask {
                            workspace.update(cx, |ws, cx| {
                                ws.open_and_ask(prompt, connection, window, cx)
                            });
                        }
                        dispatch(&commands, window, cx);
                    };
                    if command_delay > 0.0 {
                        window
                            .spawn(cx, async move |cx| {
                                cx.background_executor()
                                    .timer(std::time::Duration::from_secs_f32(command_delay))
                                    .await;
                                let _ = cx.update(later);
                            })
                            .detach();
                    } else {
                        later(window, cx);
                    }
                });
            });
        }
        Err(err) => {
            log::error!("failed to open the main window: {err:#}");
            cx.quit();
        }
    }
}

/// Dispatch `--command` actions by name. Actions without a JSON form (`no_json`) are taken from the
/// command palette's registry, which holds a default instance of each.
fn dispatch(commands: &[String], window: &mut Window, cx: &mut App) {
    for name in commands {
        let action = cx.build_action(name, None).or_else(|err| {
            ui_workspace::palette::CommandRegistry::commands(cx)
                .iter()
                .find(|c| c.action.name() == name.as_str())
                .map(|c| c.action.boxed_clone())
                .ok_or(err)
        });
        match action {
            Ok(action) => window.dispatch_action(action, cx),
            Err(err) => log::error!("--command {name:?}: {err}"),
        }
    }
}

fn open_graph_canvas(args: &Args, nodes: usize, edges: usize, cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(BENCH_WINDOW[0]), px(BENCH_WINDOW[1])),
            cx,
        ))),
        app_id: Some("gremliner".into()),
        ..Default::default()
    };
    let bench = args.render_bench.is_some();
    let perf_hud = args.perf_hud || bench;
    let seconds = args.bench_seconds;
    gpui_kit::open_window(options, cx, move |_, cx| {
        cx.new(|cx| {
            let store = graph_store::synth::generate(nodes, edges, 8, 42);
            let mut canvas = GraphCanvas::new(store, cx);
            canvas.set_perf_hud(perf_hud);
            if bench {
                canvas.start_bench(BenchRun::new(Duration::from_secs_f32(seconds), true));
            }
            canvas
        })
    })
    .expect("failed to open window");
}
