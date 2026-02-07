use notify::{recommended_watcher, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

pub struct WatcherHandle {
    _watcher: RecommendedWatcher,
    _join: thread::JoinHandle<()>,
}

pub fn start_watcher<F>(policies_dir: &str, on_change: F) -> Result<WatcherHandle, notify::Error>
where
    F: Fn(PathBuf) + Send + 'static,
{
    let (sender, receiver) = mpsc::channel::<Result<Event, notify::Error>>();
    let mut watcher = recommended_watcher(move |event| {
        let _ = sender.send(event);
    })?;

    watcher.watch(
        PathBuf::from(policies_dir).as_path(),
        RecursiveMode::NonRecursive,
    )?;

    let join = thread::spawn(move || consume_events(receiver, on_change));

    Ok(WatcherHandle {
        _watcher: watcher,
        _join: join,
    })
}

fn consume_events<F>(receiver: Receiver<Result<Event, notify::Error>>, on_change: F)
where
    F: Fn(PathBuf),
{
    for event in receiver.into_iter().flatten() {
        for path in event.paths {
            on_change(path);
        }
    }
}
