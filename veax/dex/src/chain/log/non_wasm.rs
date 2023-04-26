use std::cell::RefCell;

pub fn log_str_impl(msg: &str) {
    LOGGER.with(|l| l.borrow_mut().log(msg));
}

thread_local! {
    static LOGGER: RefCell<Box<dyn Logger>> = RefCell::new(Box::new(DefaultLogger));
}

pub struct LoggerScope {
    prev_logger: RefCell<Option<Box<dyn Logger>>>,
}

impl LoggerScope {
    pub fn new(l_new: impl Logger + 'static) -> Self {
        Self {
            prev_logger: Some(LOGGER.with(|l| l.replace(Box::new(l_new)))).into(),
        }
    }
}

impl Drop for LoggerScope {
    fn drop(&mut self) {
        LOGGER.with(|l| {
            l.replace(match self.prev_logger.replace(None) {
                Some(l) => l,
                // Never fails - LOGGER always contains something
                _ => unreachable!(),
            })
        });
    }
}

pub trait Logger {
    fn log(&mut self, s: &str);
}

pub struct DefaultLogger;

impl Logger for DefaultLogger {
    fn log(&mut self, s: &str) {
        near_sdk::env::log_str(s);
    }
}
