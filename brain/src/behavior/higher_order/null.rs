use crate::strategy::{Action, Behavior, Context};
use nameof::name_of_type;

pub struct NullBehavior;

impl NullBehavior {
    pub fn new() -> Self {
        Self
    }
}

impl Behavior for NullBehavior {
    fn name(&self) -> &str {
        name_of_type!(NullBehavior)
    }

    fn execute_old(&mut self, _ctx: &mut Context<'_>) -> Action {
        Action::Yield(common::halfway_house::PlayerInput::default())
    }
}
