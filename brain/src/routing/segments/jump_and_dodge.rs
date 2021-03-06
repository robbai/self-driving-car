use crate::{
    behavior::{higher_order::Chain, movement::Yielder},
    eeg::{color, Drawable},
    routing::models::{CarState, CarState2D, SegmentPlan, SegmentRunAction, SegmentRunner},
    strategy::{Action, Behavior, Context, Priority},
};
use common::prelude::*;
use derive_new::new;
use nalgebra::UnitComplex;
use nameof::name_of_type;

const JUMP_TIME: f32 = 6.0 / 120.0;
const WAIT_TIME: f32 = 6.0 / 120.0;
const FLOAT_TIME: f32 = 1.333333;
const DODGE_IMPULSE: f32 = 500.0; // This is inaccurate (but it's an exact minimum bound).

#[derive(Clone, new)]
pub struct JumpAndDodge {
    start: CarState,
    direction: UnitComplex<f32>,
}

impl SegmentPlan for JumpAndDodge {
    fn name(&self) -> &str {
        name_of_type!(JumpAndDodge)
    }

    fn start(&self) -> CarState {
        self.start.clone()
    }

    fn end(&self) -> CarState {
        assert!(!self.start.loc.x.is_nan());
        assert!(!self.start.vel.norm().is_nan());
        assert!(!self.direction.angle().is_nan());

        let impulse = self.direction * self.start.forward_axis_2d().into_inner() * DODGE_IMPULSE;
        let dodge_vel = self.start.vel.to_2d() + impulse;
        let loc = self.start.loc.to_2d()
            + (JUMP_TIME + WAIT_TIME) * self.start.vel.to_2d()
            + FLOAT_TIME * dodge_vel;
        CarState2D {
            loc,
            rot: self.start.rot.to_2d(),
            vel: dodge_vel,
            boost: self.start.boost,
        }
        .to_3d()
    }

    fn duration(&self) -> f32 {
        JUMP_TIME + WAIT_TIME + FLOAT_TIME
    }

    fn run(&self) -> Box<dyn SegmentRunner> {
        Box::new(JumpAndDodgeRunner::new(self.clone()))
    }

    fn draw(&self, ctx: &mut Context<'_>) {
        ctx.eeg.draw(Drawable::Line(
            self.start.loc.to_2d(),
            self.end().loc.to_2d(),
            color::GREEN,
        ));
    }
}

struct JumpAndDodgeRunner {
    behavior: Box<dyn Behavior>,
}

impl JumpAndDodgeRunner {
    pub fn new(plan: JumpAndDodge) -> Self {
        let behavior = Box::new(Chain::new(Priority::Idle, vec![
            Box::new(Yielder::new(
                JUMP_TIME,
                common::halfway_house::PlayerInput {
                    Jump: true,
                    ..Default::default()
                },
            )),
            Box::new(Yielder::new(
                WAIT_TIME,
                common::halfway_house::PlayerInput {
                    ..Default::default()
                },
            )),
            Box::new(Yielder::new(
                6.0 / 120.0,
                common::halfway_house::PlayerInput {
                    Pitch: -plan.direction.cos_angle(),
                    Yaw: plan.direction.sin_angle(),
                    Jump: true,
                    ..Default::default()
                },
            )),
            Box::new(Yielder::new(
                FLOAT_TIME - 6.0 / 120.0,
                common::halfway_house::PlayerInput {
                    ..Default::default()
                },
            )),
        ]));
        Self { behavior }
    }
}

impl SegmentRunner for JumpAndDodgeRunner {
    fn name(&self) -> &str {
        name_of_type!(JumpAndDodgeRunner)
    }

    fn execute_old(&mut self, ctx: &mut Context<'_>) -> SegmentRunAction {
        match self.behavior.execute_old(ctx) {
            Action::Yield(i) => SegmentRunAction::Yield(i),
            Action::TailCall(_) => panic!("TailCall not yet supported in SegmentRunner"),
            Action::RootCall(_) => SegmentRunAction::Failure,
            Action::Return => SegmentRunAction::Success,
            Action::Abort => SegmentRunAction::Failure,
        }
    }
}
