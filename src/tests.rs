use std::sync::Arc;
use std::{rc::Rc, sync::MutexGuard};

use ncp_matcher::Config;

use crate::pattern::{CaseMatching, Normalization};
use crate::{MatchListConfig, Nucleo};

fn wait_for_worker(nucleo: &mut Nucleo<String>) {
    while nucleo.tick(100).running {}
}

#[test]
fn nucleo_type_does_not_require_thread_safe_items() {
    fn accepts_non_send(_: Option<Nucleo<Rc<()>>>) {}
    fn accepts_non_static<'a>(_: Option<Nucleo<MutexGuard<'a, ()>>>) {}

    accepts_non_send(None);
    accepts_non_static(None);
}

#[test]
fn active_injector_count() {
    let mut nucleo: Nucleo<()> = Nucleo::new(Config::DEFAULT, Arc::new(|| ()), Some(1), 1);
    assert_eq!(nucleo.active_injectors(), 0);
    let injector = nucleo.injector();
    assert_eq!(nucleo.active_injectors(), 1);
    let injector2 = nucleo.injector();
    assert_eq!(nucleo.active_injectors(), 2);
    drop(injector2);
    assert_eq!(nucleo.active_injectors(), 1);
    nucleo.restart(false);
    assert_eq!(nucleo.active_injectors(), 0);
    let injector3 = nucleo.injector();
    assert_eq!(nucleo.active_injectors(), 1);
    nucleo.tick(0);
    assert_eq!(nucleo.active_injectors(), 1);
    drop(injector);
    assert_eq!(nucleo.active_injectors(), 1);
    drop(injector3);
    assert_eq!(nucleo.active_injectors(), 0);
}

#[test]
fn configuration_changes_schedule_worker_updates() {
    let mut nucleo = Nucleo::new(Config::DEFAULT, Arc::new(|| ()), Some(1), 1);
    let injector = nucleo.injector();
    injector.push("xab".to_owned(), |item, columns| {
        columns[0] = item.as_str().into();
    });
    injector.push("xxab".to_owned(), |item, columns| {
        columns[0] = item.as_str().into();
    });
    nucleo
        .pattern
        .reparse(0, "ab", CaseMatching::Smart, Normalization::Smart, false);
    wait_for_worker(&mut nucleo);

    let old_score = nucleo.snapshot().matches()[0].score;
    let mut config = Config::DEFAULT;
    config.prefer_prefix = true;
    nucleo.update_config(config);
    wait_for_worker(&mut nucleo);
    assert!(nucleo.snapshot().matches()[0].score > old_score);

    nucleo.sort_results(false);
    wait_for_worker(&mut nucleo);
    nucleo.reverse_items(true);
    wait_for_worker(&mut nucleo);
    assert_eq!(
        nucleo
            .snapshot()
            .matches()
            .iter()
            .map(|match_| match_.idx)
            .collect::<Vec<_>>(),
        [1, 0]
    );
}

#[test]
fn reverse_items_applies_to_empty_pattern() {
    let match_list_config = MatchListConfig {
        sort_results: true,
        reverse_items: true,
    };
    let mut nucleo = Nucleo::with_match_list_config(
        Config::DEFAULT,
        Arc::new(|| ()),
        Some(1),
        1,
        match_list_config,
    );
    let injector = nucleo.injector();
    injector.push("first".to_owned(), |item, columns| {
        columns[0] = item.as_str().into();
    });
    injector.push("second".to_owned(), |item, columns| {
        columns[0] = item.as_str().into();
    });
    wait_for_worker(&mut nucleo);

    assert_eq!(
        nucleo
            .snapshot()
            .matches()
            .iter()
            .map(|match_| match_.idx)
            .collect::<Vec<_>>(),
        [1, 0]
    );
}
