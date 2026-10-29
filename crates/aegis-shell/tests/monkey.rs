//! The window under abuse (job 056): every callback the window offers is
//! fired at random with hostile values, over a real server, while the window
//! changes size, and the app must neither panic nor stop drawing.
//!
//! "Hostile" means what a user, a script or a bug can send: empty and very
//! long text, control characters and right to left marks, indexes that do not
//! exist (negative, huge), floats that are NaN or infinite, buttons pressed
//! in the wrong order (remove with nothing selected, undo before edit),
//! pages that have nothing to show, and windows from 240 x 160 up to 3000
//! pixels wide.
//!
//! The run is deterministic for a seed. `AEGIS_MONKEY_STEPS` sets the steps
//! per run (default 300; a local run uses 5000 or more) and `AEGIS_MONKEY_SEEDS`
//! the number of seeds (default 2).

mod common;

use std::time::Duration;

use aegis_client::state::Mode;
use aegis_shell::AppWindow;
use common::{App, TestServer};

/// A small deterministic generator (xorshift64*), so a failure can be
/// replayed from its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn flag(&mut self) -> bool {
        self.below(2) == 0
    }

    /// An index: mostly small and valid, sometimes wrong in every way.
    fn int(&mut self) -> i32 {
        const ODD: [i32; 8] = [-1, -2, i32::MIN, i32::MAX, 1000, 99_999, 255, 65_536];
        if self.below(5) == 0 {
            ODD[self.below(ODD.len() as u64) as usize]
        } else {
            self.below(8) as i32
        }
    }

    fn float(&mut self) -> f32 {
        const ODD: [f32; 9] = [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.0,
            1e30,
            -1e30,
            f32::MIN_POSITIVE,
            0.0,
            -1.0,
        ];
        if self.below(6) == 0 {
            ODD[self.below(ODD.len() as u64) as usize]
        } else {
            (self.below(2400) as f32) - 200.0
        }
    }

    fn text(&mut self) -> String {
        const PIECES: [&str; 14] = [
            "",
            " ",
            "x",
            "Brake ECU",
            "Spoofed brake request",
            "ü ß 日本語 🚗",
            "\u{202e}reversed\u{202c}",
            "line one\nline two\r\n\ttabbed",
            "\u{0}\u{7}\u{1b}[31mred",
            "<b>&amp;</b> \"quoted\" 'single'",
            "../../etc/passwd",
            "OR Goal\n  AND Gate\n    - Step\n    - Step\n  - Other",
            "OR Goal\n  - A\n  - A\n    - under a step",
            "   ",
        ];
        match self.below(8) {
            0 => "long ".repeat(2000),
            1 => "W".repeat(10_000),
            2 => (0..self.below(40)).map(|_| "word ").collect(),
            _ => PIECES[self.below(PIECES.len() as u64) as usize].to_owned(),
        }
    }
}

/// Every callback of the window with random arguments, except the ones that
/// leave the session (log out, change server), the exports (they write
/// files; the page tests cover them), and closing the project or ending the
/// edit session, which `abuse` does itself now and then: at random they end
/// the run's editing in the first few steps and nothing else is tested. The
/// same goes for the login-level buttons (retry, log in, connect): the window
/// has no login hooks here, and pressing retry over and over keeps the
/// session reconnecting, so `abuse` presses them rarely instead.
#[allow(clippy::type_complexity)]
fn actions() -> Vec<fn(&AppWindow, &mut Rng)> {
    vec![
        |w, _r| {
            w.invoke_retry();
        },
        |w, _r| {
            w.invoke_log_in();
        },
        |w, r| {
            w.invoke_connect_server(r.text().into(), r.int());
        },
        |w, r| {
            w.invoke_password_log_in(r.text().into(), r.text().into(), r.flag());
        },
        |w, r| {
            w.invoke_dev_log_in(r.text().into());
        },
        |w, _r| {
            w.invoke_use_password_login();
        },
        |w, _r| {
            w.invoke_use_company_login();
        },
        |w, _r| {
            w.invoke_check_for_updates();
        },
        |w, _r| {
            w.invoke_download_update();
        },
        |w, r| {
            w.invoke_change_password(r.text().into(), r.text().into());
        },
        |w, r| {
            w.invoke_open_project(r.text().into());
        },
        |w, r| {
            w.invoke_create_project(r.text().into());
        },
        |w, _r| {
            w.invoke_refresh_projects();
        },
        |w, _r| {
            w.invoke_close_project();
        },
        |w, r| {
            w.invoke_rename_project(r.text().into());
        },
        |w, _r| {
            w.invoke_delete_project();
        },
        |w, r| {
            w.invoke_show_page(r.int());
        },
        |w, _r| {
            w.invoke_begin_edit();
        },
        |w, _r| {
            w.invoke_end_edit();
        },
        |w, _r| {
            w.invoke_undo();
        },
        |w, _r| {
            w.invoke_redo();
        },
        |w, _r| {
            w.invoke_create_sample();
        },
        |w, r| {
            w.invoke_set_residual(r.flag());
        },
        |w, r| {
            w.invoke_dismiss_notice(r.int());
        },
        |w, r| {
            w.invoke_ov_top_clicked(r.int());
        },
        |w, r| {
            w.invoke_pick_component(r.int());
        },
        |w, r| {
            w.invoke_add_component(r.text().into());
        },
        |w, r| {
            w.invoke_rename_component(r.text().into());
        },
        |w, r| {
            w.invoke_set_component_parent(r.int());
        },
        |w, _r| {
            w.invoke_remove_component();
        },
        |w, r| {
            w.invoke_pick_interface(r.int());
        },
        |w, r| {
            w.invoke_add_interface(r.text().into(), r.int(), r.int());
        },
        |w, r| {
            w.invoke_rename_interface(r.text().into());
        },
        |w, r| {
            w.invoke_set_interface_kind(r.int());
        },
        |w, _r| {
            w.invoke_remove_interface();
        },
        |w, r| {
            w.invoke_pick_data_flow(r.int());
        },
        |w, r| {
            w.invoke_add_data_flow(r.text().into(), r.int(), r.int());
        },
        |w, r| {
            w.invoke_rename_data_flow(r.text().into());
        },
        |w, r| {
            w.invoke_set_data_flow_interface(r.int());
        },
        |w, _r| {
            w.invoke_remove_data_flow();
        },
        |w, r| {
            w.invoke_pick_trust_boundary(r.int());
        },
        |w, r| {
            w.invoke_add_trust_boundary(r.text().into());
        },
        |w, r| {
            w.invoke_rename_trust_boundary(r.text().into());
        },
        |w, r| {
            w.invoke_toggle_trust_boundary_member(r.int(), r.flag());
        },
        |w, _r| {
            w.invoke_remove_trust_boundary();
        },
        |w, r| {
            w.invoke_pick_asset(r.int());
        },
        |w, r| {
            w.invoke_add_asset(r.text().into());
        },
        |w, r| {
            w.invoke_rename_asset(r.text().into());
        },
        |w, r| {
            w.invoke_set_asset_component(r.int());
        },
        |w, r| {
            w.invoke_toggle_asset_property(r.int(), r.flag());
        },
        |w, _r| {
            w.invoke_remove_asset();
        },
        |w, r| {
            w.invoke_asset_threat_clicked(r.int());
        },
        |w, r| {
            w.invoke_pick_damage(r.int());
        },
        |w, r| {
            w.invoke_add_damage(r.text().into());
        },
        |w, r| {
            w.invoke_rename_damage(r.text().into());
        },
        |w, r| {
            w.invoke_set_impact(r.int(), r.int());
        },
        |w, _r| {
            w.invoke_remove_damage();
        },
        |w, r| {
            w.invoke_damage_threat_clicked(r.int());
        },
        |w, r| {
            w.invoke_select_threat(r.int());
        },
        |w, r| {
            w.invoke_add_threat(r.text().into(), r.int(), r.int());
        },
        |w, r| {
            w.invoke_rename_threat(r.text().into());
        },
        |w, r| {
            w.invoke_set_threat_category(r.int());
        },
        |w, r| {
            w.invoke_toggle_threat_damage(r.int(), r.flag());
        },
        |w, r| {
            w.invoke_add_catalog_ref(r.int(), r.text().into());
        },
        |w, r| {
            w.invoke_remove_catalog_ref(r.int());
        },
        |w, r| {
            w.invoke_set_threat_feasibility(r.int());
        },
        |w, r| {
            w.invoke_set_threat_tree(r.int());
        },
        |w, _r| {
            w.invoke_new_tree_for_threat();
        },
        |w, _r| {
            w.invoke_open_threat_tree();
        },
        |w, _r| {
            w.invoke_remove_threat();
        },
        |w, r| {
            w.invoke_set_treatment(r.int());
        },
        |w, r| {
            w.invoke_set_threat_goal(r.int());
        },
        |w, r| {
            w.invoke_set_threat_claim(r.int());
        },
        |w, r| {
            w.invoke_pick_risk(r.int());
        },
        |w, r| {
            w.invoke_pick_tree(r.int());
        },
        |w, r| {
            w.invoke_new_tree(r.text().into());
        },
        |w, r| {
            w.invoke_apply_outline(r.text().into());
        },
        |w, _r| {
            w.invoke_remove_tree();
        },
        |w, r| {
            w.invoke_rename_tree_node(r.text().into());
        },
        |w, r| {
            w.invoke_set_tree_node_gate(r.int());
        },
        |w, r| {
            w.invoke_add_tree_child(r.int(), r.text().into());
        },
        |w, r| {
            w.invoke_branch_step_to_gate(r.int(), r.text().into());
        },
        |w, r| {
            w.invoke_insert_parent_gate(r.int(), r.text().into());
        },
        |w, r| {
            w.invoke_add_sibling_node(r.int(), r.text().into());
        },
        |w, r| {
            w.invoke_move_tree_node(r.int());
        },
        |w, _r| {
            w.invoke_remove_tree_node();
        },
        |w, r| {
            w.invoke_attach_control(r.int(), r.int());
        },
        |w, r| {
            w.invoke_set_mitigation(r.int(), r.int());
        },
        |w, r| {
            w.invoke_detach_control(r.int());
        },
        |w, _r| {
            w.invoke_select_threat_of_tree();
        },
        |w, r| {
            w.invoke_set_rating(r.int());
        },
        |w, r| {
            w.invoke_set_factor(r.int(), r.int());
        },
        |w, r| {
            w.invoke_canvas_down(r.float(), r.float());
        },
        |w, r| {
            w.invoke_canvas_move(r.float(), r.float());
        },
        |w, r| {
            w.invoke_canvas_up(r.float(), r.float());
        },
        |w, r| {
            w.invoke_canvas_scroll(r.float(), r.float(), r.float());
        },
        |w, r| {
            let _ = w.invoke_canvas_key(r.text().into(), r.flag());
        },
        |w, r| {
            w.invoke_canvas_resized(r.float(), r.float());
        },
        |w, r| {
            w.invoke_canvas_node_activated(r.int());
        },
        |w, _r| {
            w.invoke_fit();
        },
        |w, _r| {
            w.invoke_zoom_reset();
        },
        |w, r| {
            w.invoke_pick_control(r.int());
        },
        |w, r| {
            w.invoke_add_control(r.text().into());
        },
        |w, r| {
            w.invoke_rename_control(r.text().into());
        },
        |w, _r| {
            w.invoke_remove_control();
        },
        |w, r| {
            w.invoke_control_step_clicked(r.int());
        },
        |w, r| {
            w.invoke_control_goal_clicked(r.int());
        },
        |w, r| {
            w.invoke_pick_goal(r.int());
        },
        |w, r| {
            w.invoke_add_goal(r.text().into());
        },
        |w, r| {
            w.invoke_rename_goal(r.text().into());
        },
        |w, r| {
            w.invoke_toggle_goal_control(r.int(), r.flag());
        },
        |w, _r| {
            w.invoke_remove_goal();
        },
        |w, r| {
            w.invoke_goal_threat_clicked(r.int());
        },
        |w, r| {
            w.invoke_pick_claim(r.int());
        },
        |w, r| {
            w.invoke_add_claim(r.text().into());
        },
        |w, r| {
            w.invoke_rename_claim(r.text().into());
        },
        |w, _r| {
            w.invoke_remove_claim();
        },
        |w, r| {
            w.invoke_claim_threat_clicked(r.int());
        },
        |w, r| {
            w.invoke_problem_clicked(r.int());
        },
        |w, r| {
            w.invoke_pick_baseline(r.int());
        },
        |w, r| {
            w.invoke_create_baseline(r.text().into());
        },
        |w, _r| {
            w.invoke_compare_baseline();
        },
        |w, _r| {
            w.invoke_stop_compare();
        },
        |w, _r| {
            w.invoke_refresh_history();
        },
        |w, r| {
            w.invoke_pick_user(r.int());
        },
        |w, r| {
            w.invoke_create_user(r.text().into(), r.text().into(), r.text().into(), r.flag());
        },
        |w, _r| {
            w.invoke_toggle_user_admin();
        },
        |w, _r| {
            w.invoke_toggle_user_disabled();
        },
        |w, r| {
            w.invoke_reset_user_password(r.text().into());
        },
        |w, _r| {
            w.invoke_delete_user();
        },
        |w, _r| {
            w.invoke_refresh_sessions();
        },
        |w, r| {
            w.invoke_pick_session(r.int());
        },
        |w, _r| {
            w.invoke_revoke_session();
        },
        |w, _r| {
            w.invoke_break_lock();
        },
        |w, r| {
            w.invoke_set_password_login(r.flag());
        },
        |w, r| {
            w.invoke_set_event_filter(r.int());
        },
        |w, _r| {
            w.invoke_refresh_events();
        },
        |w, _r| {
            w.invoke_verify_audit();
        },
    ]
}

/// Changes in the sample TARA (`create_sample`).
const SAMPLE_CHANGES: u64 = 16;

const SIZES: [(u32, u32); 9] = [
    (1360, 860),
    (1100, 760),
    (800, 600),
    (640, 480),
    (400, 300),
    (240, 160),
    (1920, 1080),
    (3000, 1400),
    (1360, 860),
];

fn steps() -> u64 {
    std::env::var("AEGIS_MONKEY_STEPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300)
}

fn seeds() -> u64 {
    std::env::var("AEGIS_MONKEY_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2)
}

/// Brings the window back to work like a persistent user would: waits until
/// the session is online, opens a project again when it was deleted or
/// closed, takes the edit lock again when it is gone, and refills an empty
/// project. Gives up quietly after three seconds (the storm may have left
/// the app somewhere it cannot leave; the other checks still apply).
async fn settle(app: &App, name: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        app.controller.apply_latest();
        let view = app.session.view();
        let online = matches!(
            view.connection,
            aegis_client::state::ConnectionView::Online { .. }
        );
        match (&view.project, online) {
            (_, false) => {}
            (None, true) => app.window.invoke_create_project(name.into()),
            (Some(p), true) if p.mode != Mode::Editing => app.window.invoke_begin_edit(),
            (Some(p), true) if p.is_empty => app.window.invoke_create_sample(),
            (Some(p), true) if p.sync == aegis_client::state::SyncView::Synced => return,
            (Some(_), true) => {}
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Runs `steps` random actions, redrawing now and then and settling the
/// session every 50 steps. Returns how many of the changes that reached the
/// server were the storm's own (the sample project it refills is not counted).
async fn abuse(app: &mut App, seed: u64, steps: u64) -> u64 {
    let actions = actions();
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    // Changes confirmed by the server, counted per project (a project that
    // the storm deleted starts the count again for the next one).
    let (mut project, mut last, mut confirmed) = (String::new(), 0u64, 0u64);
    for step in 0..steps {
        if step % 50 == 0 {
            settle(app, &format!("Chaos {seed}-{step}")).await;
            if rng.below(4) == 0 {
                // Give the lock up on purpose, now and then.
                app.window.invoke_end_edit();
            }
        }
        if rng.below(150) == 0 {
            match rng.below(6) {
                0 => app.window.invoke_retry(),
                1 => app.window.invoke_log_in(),
                2 => app
                    .window
                    .invoke_connect_server(rng.text().into(), rng.int()),
                3 => app.window.invoke_password_log_in(
                    rng.text().into(),
                    rng.text().into(),
                    rng.flag(),
                ),
                4 => app.window.invoke_use_password_login(),
                _ => app.window.invoke_use_company_login(),
            }
        }
        let action = actions[rng.below(actions.len() as u64) as usize];
        action(&app.window, &mut rng);
        // Move around the pages, which have different content to break.
        if rng.below(12) == 0 {
            app.window.invoke_show_page(rng.below(14) as i32);
        }
        if rng.below(40) == 0 {
            let (w, h) = SIZES[rng.below(SIZES.len() as u64) as usize];
            app.resize(w, h);
        }
        if step % 15 == 0 {
            let frame = app.frame();
            assert_eq!(
                frame.rgb.len() as u32,
                frame.width * frame.height * 3,
                "seed {seed} step {step}: a frame the wrong size"
            );
        }
        if step % 10 == 0 {
            // Let the session answer the commands sent so far.
            tokio::time::sleep(Duration::from_millis(3)).await;
            app.controller.apply_latest();
            if std::env::var_os("AEGIS_MONKEY_TRACE").is_some() {
                let v = app.session.view();
                eprintln!(
                    "step {step}: project {:?} mode {:?} seq {:?} sync {:?} undo {:?}",
                    v.project.as_ref().map(|p| p.entry.name.clone()),
                    v.project.as_ref().map(|p| p.mode),
                    v.project.as_ref().map(|p| p.confirmed_seq),
                    v.project.as_ref().map(|p| p.sync),
                    v.project.as_ref().map(|p| p.can_undo),
                );
            }
            if let Some(p) = &app.session.view().project {
                if p.entry.name != project || p.confirmed_seq < last {
                    // Another project, or the same one reloaded: start again.
                    project = p.entry.name.clone();
                    last = p.confirmed_seq;
                } else if p.confirmed_seq > last {
                    // Only changes made after the sample was in count as the
                    // storm's own (the sample is SAMPLE_CHANGES of them).
                    if last >= SAMPLE_CHANGES {
                        confirmed += p.confirmed_seq - last;
                    }
                    last = p.confirmed_seq;
                }
            }
        }
    }
    confirmed
}

#[tokio::test(flavor = "current_thread")]
async fn a_window_with_no_server_survives_every_callback() {
    // Nothing listens here: the window stays on "connecting" and retries.
    let mut config = aegis_client::session::SessionConfig::new("nobody");
    config.retry_delay = Duration::from_millis(50);
    for seed in 1..=seeds() {
        let (session, _task) = aegis_client::session::Session::start(
            "http://127.0.0.1:9",
            std::sync::Arc::new(aegis_client::connection::DevLogin::new("nobody", false)),
            config.clone(),
        );
        let mut app = App::new(session, 1100, 760);
        let _ = abuse(&mut app, seed, steps()).await;
        let frame = app.frame();
        assert!(
            frame.rgb.iter().any(|b| *b != frame.rgb[0]),
            "seed {seed}: the window is still drawn"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn an_editor_with_a_project_survives_every_callback() {
    let server = TestServer::start().await;
    for seed in 1..=seeds() {
        let mut app = App::new(server.admin_session("mira"), 1360, 860);
        app.online().await;
        app.window
            .invoke_create_project(format!("Chaos {seed}").into());
        app.until("project open", |vm| vm.project.is_some()).await;
        app.window.invoke_begin_edit();
        app.until("editing", |vm| {
            vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
        })
        .await;
        app.window.invoke_create_sample();
        app.until("sample saved", |vm| {
            vm.project.as_ref().is_some_and(|p| {
                p.tree.is_some() && p.sync == aegis_client::state::SyncView::Synced
            })
        })
        .await;

        let changes = abuse(&mut app, seed, steps()).await;

        // The storm reached real code: it made and saved changes of its own.
        eprintln!("seed {seed}: {changes} changes of the storm's own reached the server");
        assert!(
            changes > 0,
            "seed {seed}: no edit got through; the monkey is not reaching the model"
        );

        // Still alive: it draws, the session answers, and a normal user can
        // go on working after the storm.
        let frame = app.frame();
        assert!(
            frame.rgb.iter().any(|b| *b != frame.rgb[0]),
            "seed {seed}: the window is still drawn"
        );
        app.resize(1360, 860);
        app.window.invoke_show_page(0);
        app.window.invoke_refresh_projects();
        app.until("still answering", |vm| !vm.projects.is_empty())
            .await;
        let after = app.frame();
        common::save(&after, &format!("m{seed}-after-the-monkey"));
    }
    server.stop().await;
}
