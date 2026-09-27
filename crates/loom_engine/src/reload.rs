//! Scene hot reload (window mode with `--scene`), see `docs/architecture.md`
//! section 10. The watcher is pure logic: the caller passes in the current
//! time and closures that stat and read the file, so every rule can be unit
//! tested without a file system or a window. It runs outside the simulation
//! and never affects determinism.

use std::io;
use std::time::{Duration, Instant, SystemTime};

use crate::sim::LoadedScene;

/// How often the file's modification time is checked.
pub(crate) const CHECK_INTERVAL: Duration = Duration::from_millis(500);

/// What one [`SceneWatcher::poll`] found.
pub(crate) enum Poll {
    /// Less than [`CHECK_INTERVAL`] since the last check; nothing was looked at.
    NotChecked,
    /// Same modification time, same content as the running scene, or the
    /// file is momentarily missing (editors often delete or rename on save).
    Unchanged,
    /// The file holds a new valid scene. The caller restarts with it.
    Reloaded(Box<LoadedScene>),
    /// New content that fails to parse or validate. Keep the running scene,
    /// log the error and count it.
    Failed(anyhow::Error),
    /// The same broken content as the previous failure: not counted again.
    FailedAgain,
}

pub(crate) struct SceneWatcher {
    last_check: Instant,
    /// Modification time of the last content read (or seen at start).
    modified: Option<SystemTime>,
    /// Source text of the running scene.
    current: String,
    /// Content (or IO error) of the last failed reload; cleared on success.
    last_failure: Option<String>,
}

impl SceneWatcher {
    /// `modified` and `source` describe the scene the run started with.
    pub fn new(now: Instant, modified: Option<SystemTime>, source: String) -> Self {
        Self {
            last_check: now,
            modified,
            current: source,
            last_failure: None,
        }
    }

    pub fn poll(
        &mut self,
        now: Instant,
        stat: impl FnOnce() -> io::Result<SystemTime>,
        read: impl FnOnce() -> io::Result<String>,
    ) -> Poll {
        if now.duration_since(self.last_check) < CHECK_INTERVAL {
            return Poll::NotChecked;
        }
        self.last_check = now;

        let modified = match stat() {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Poll::Unchanged,
            Err(e) => return self.fail(format!("io: {e}"), anyhow::Error::new(e)),
        };
        if self.modified == Some(modified) {
            return Poll::Unchanged;
        }
        let content = match read() {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Poll::Unchanged,
            Err(e) => return self.fail(format!("io: {e}"), anyhow::Error::new(e)),
        };
        // Remember the time only once its content has been seen.
        self.modified = Some(modified);
        if content == self.current {
            return Poll::Unchanged;
        }
        match LoadedScene::from_source(&content) {
            Ok(loaded) => {
                self.current = content;
                self.last_failure = None;
                Poll::Reloaded(Box::new(loaded))
            }
            Err(e) => self.fail(content, e),
        }
    }

    /// Records a failure keyed by `key` (the broken content, or the IO error
    /// text); the same key twice in a row is reported only once.
    fn fail(&mut self, key: String, error: anyhow::Error) -> Poll {
        if self.last_failure.as_deref() == Some(key.as_str()) {
            return Poll::FailedAgain;
        }
        self.last_failure = Some(key);
        Poll::Failed(error)
    }
}

/// Hot reload keeps the running game: a reloaded scene naming another game
/// is rejected like an invalid one (`docs/architecture.md` section 10).
/// Switching would also switch the bindings, the replayed script and the
/// mouse capture.
pub(crate) fn check_same_game(running: &str, reloaded: &str) -> anyhow::Result<()> {
    if running == reloaded {
        return Ok(());
    }
    anyhow::bail!(
        "hot reload cannot switch games: the scene now says {reloaded:?}, but {running:?} is running; restart loom to switch"
    )
}

/// Stats the scene file for [`SceneWatcher::poll`].
pub(crate) fn modified_time(path: &std::path::Path) -> io::Result<SystemTime> {
    std::fs::metadata(path)?.modified()
}

/// Reads the scene file for [`SceneWatcher::poll`].
pub(crate) fn read_source(path: &std::path::Path) -> io::Result<String> {
    std::fs::read_to_string(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const GOOD: &str = include_str!("../../../games/souls/src/demo.scene.ron");

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn not_found() -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, "gone")
    }

    /// A watcher started at `t0` on `GOOD` with modification time 1. The
    /// working directory becomes the repository root, as for `loom`: the
    /// souls HUD's icon sheet path is relative to it. Every test in this
    /// binary that depends on the working directory wants that root.
    fn watcher(t0: Instant) -> SceneWatcher {
        static ROOT: std::sync::Once = std::sync::Once::new();
        ROOT.call_once(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            std::env::set_current_dir(root).expect("the repository root exists");
        });
        SceneWatcher::new(t0, Some(at(1)), GOOD.to_owned())
    }

    /// `GOOD` plus a node `badge` showing `tag` of the project's tile sheet
    /// (an absolute path, so the test does not depend on the working
    /// directory).
    fn with_sprite(tag: &str) -> String {
        let sheet = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/art/tiles.json")
            .display()
            .to_string()
            .replace('\\', "/");
        let badge = format!("(name: \"badge\", sprite: (sheet: \"{sheet}\", tag: \"{tag}\")),");
        GOOD.replacen(
            "(name: \"ground\",",
            &format!("{badge}\n        (name: \"ground\","),
            1,
        )
    }

    fn moved_player() -> String {
        GOOD.replace(
            r#"(name: "player", position: (0.0, 0.0, 0.0)"#,
            r#"(name: "player", position: (3.0, 0.0, 0.0)"#,
        )
    }

    fn poll_with(w: &mut SceneWatcher, now: Instant, modified: u64, content: &str) -> Poll {
        w.poll(now, || Ok(at(modified)), || Ok(content.to_owned()))
    }

    #[test]
    fn watcher_waits_half_a_second_between_checks() {
        let t0 = Instant::now();
        let mut w = watcher(t0);
        let stats = Cell::new(0);
        let stat = || {
            stats.set(stats.get() + 1);
            Ok(at(1))
        };
        assert!(matches!(
            w.poll(t0 + Duration::from_millis(400), stat, || Ok(String::new())),
            Poll::NotChecked
        ));
        assert_eq!(stats.get(), 0);
        let stat = || {
            stats.set(stats.get() + 1);
            Ok(at(1))
        };
        assert!(matches!(
            w.poll(t0 + CHECK_INTERVAL, stat, || Ok(String::new())),
            Poll::Unchanged
        ));
        assert_eq!(stats.get(), 1);
    }

    #[test]
    fn unchanged_modification_time_skips_reading_the_file() {
        let t0 = Instant::now();
        let mut w = watcher(t0);
        let reads = Cell::new(0);
        let read = || {
            reads.set(reads.get() + 1);
            Ok(moved_player())
        };
        assert!(matches!(
            w.poll(t0 + CHECK_INTERVAL, || Ok(at(1)), read),
            Poll::Unchanged
        ));
        assert_eq!(reads.get(), 0);
    }

    #[test]
    fn identical_content_with_new_modification_time_is_not_a_reload() {
        let t0 = Instant::now();
        let mut w = watcher(t0);
        assert!(matches!(
            poll_with(&mut w, t0 + CHECK_INTERVAL, 2, GOOD),
            Poll::Unchanged
        ));
    }

    #[test]
    fn missing_file_is_retried_without_an_error() {
        let t0 = Instant::now();
        let mut w = watcher(t0);
        // Deleted during an editor's save: stat or read may report it missing.
        assert!(matches!(
            w.poll(
                t0 + CHECK_INTERVAL,
                || Err(not_found()),
                || Ok(String::new())
            ),
            Poll::Unchanged
        ));
        assert!(matches!(
            w.poll(t0 + CHECK_INTERVAL * 2, || Ok(at(2)), || Err(not_found())),
            Poll::Unchanged
        ));
        // Back with new content, even under the same new time: reloads.
        assert!(matches!(
            poll_with(&mut w, t0 + CHECK_INTERVAL * 3, 2, &moved_player()),
            Poll::Reloaded(_)
        ));
    }

    #[test]
    fn changed_valid_scene_reloads() {
        let t0 = Instant::now();
        let mut w = watcher(t0);
        let Poll::Reloaded(loaded) = poll_with(&mut w, t0 + CHECK_INTERVAL, 2, &moved_player())
        else {
            panic!("expected a reload");
        };
        let game = &loaded.game;
        let crate::games::AnyGameState::Souls(state) = game.state() else {
            panic!("the demo scene is a souls scene");
        };
        assert_eq!(state.player.position, glam::Vec3::new(3.0, 0.0, 0.0));
        assert_eq!(game.clock().tick(), 0);
        // The new content is now current: touching it again is no reload.
        assert!(matches!(
            poll_with(&mut w, t0 + CHECK_INTERVAL * 2, 3, &moved_player()),
            Poll::Unchanged
        ));
    }

    #[test]
    fn broken_scene_counts_one_error_until_its_content_changes() {
        let t0 = Instant::now();
        let mut w = watcher(t0);
        let broken = &GOOD[..GOOD.len() - 2]; // drop the final ")\n"
        let other_broken = GOOD.replace("version: 1", "version: 7");
        let step = |n: u32| t0 + CHECK_INTERVAL * n;
        assert!(matches!(
            poll_with(&mut w, step(1), 2, broken),
            Poll::Failed(_)
        ));
        assert!(matches!(
            poll_with(&mut w, step(2), 3, broken),
            Poll::FailedAgain
        ));
        assert!(matches!(
            poll_with(&mut w, step(3), 4, &other_broken),
            Poll::Failed(_)
        ));
        // A good reload clears the memory: the first broken content counts again.
        assert!(matches!(
            poll_with(&mut w, step(4), 5, &moved_player()),
            Poll::Reloaded(_)
        ));
        assert!(matches!(
            poll_with(&mut w, step(5), 6, broken),
            Poll::Failed(_)
        ));
    }

    #[test]
    fn scene_with_an_unknown_sprite_tag_fails_to_reload() {
        let t0 = Instant::now();
        let mut w = watcher(t0);
        let Poll::Failed(e) = poll_with(&mut w, t0 + CHECK_INTERVAL, 2, &with_sprite("playr"))
        else {
            panic!("expected a failure");
        };
        let error = format!("{e:#}");
        assert!(error.contains("node badge: sprite"), "{error}");
        assert!(error.contains("has no tag \"playr\""), "{error}");
        // The same scene with a tag the sheet has reloads.
        assert!(matches!(
            poll_with(&mut w, t0 + CHECK_INTERVAL * 2, 3, &with_sprite("player")),
            Poll::Reloaded(_)
        ));
    }

    #[test]
    fn hot_reload_that_switches_games_is_rejected() {
        assert!(check_same_game("souls", "souls").is_ok());
        let error = check_same_game("souls", "rogue").unwrap_err();
        assert_eq!(
            error.to_string(),
            "hot reload cannot switch games: the scene now says \"rogue\", but \"souls\" is running; restart loom to switch"
        );
    }

    #[test]
    fn scene_without_character_fails_to_reload() {
        let t0 = Instant::now();
        let mut w = watcher(t0);
        let no_character = GOOD.replace("character: (run_speed: 4.0, turn_speed: 720.0),", "");
        let Poll::Failed(e) = poll_with(&mut w, t0 + CHECK_INTERVAL, 2, &no_character) else {
            panic!("expected a failure");
        };
        assert!(e.to_string().contains("character"), "{e}");
    }

    #[test]
    fn other_io_errors_fail_once_until_they_change() {
        let t0 = Instant::now();
        let mut w = watcher(t0);
        let denied = || Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        assert!(matches!(
            w.poll(t0 + CHECK_INTERVAL, denied, || Ok(String::new())),
            Poll::Failed(_)
        ));
        assert!(matches!(
            w.poll(t0 + CHECK_INTERVAL * 2, denied, || Ok(String::new())),
            Poll::FailedAgain
        ));
    }
}
