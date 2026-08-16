use std::sync::Arc;
use std::{rc::Rc, sync::MutexGuard};

use ncp_matcher::Config;

use crate::Nucleo;

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
