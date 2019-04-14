use crate::{
    behavior::movement::GetToFlatGround,
    eeg::{color, Drawable},
    routing::models::{CarState, CarState2D, SegmentPlan, SegmentRunAction, SegmentRunner},
    strategy::Context,
};
use common::{physics::CAR_LOCAL_FORWARD_AXIS_2D, prelude::*};
use nalgebra::{Point2, Unit, UnitComplex, Vector2};
use nameof::name_of_type;
use std::f32::consts::PI;

#[derive(Clone)]
pub struct SimpleArc {
    center: Point2<f32>,
    radius: f32,
    start_loc: Point2<f32>,
    start_vel: Vector2<f32>,
    start_boost: f32,
    sweep: f32,
}

pub enum SimpleArcError {
    VelocityTooLow,
    WrongGeometry,
}

impl SimpleArcError {
    pub fn to_str(&self) -> &'static str {
        match self {
            SimpleArcError::VelocityTooLow => stringify!(SimpleArcError::VelocityTooLow),
            SimpleArcError::WrongGeometry => stringify!(SimpleArcError::WrongGeometry),
        }
    }
}

impl SimpleArc {
    pub fn new(
        center: Point2<f32>,
        radius: f32,
        start_loc: Point2<f32>,
        start_vel: Vector2<f32>,
        start_boost: f32,
        end_loc: Point2<f32>,
    ) -> Result<Self, SimpleArcError> {
        // This assumes a constant speed and will estimate a ridiculous duration if the
        // velocity is too low.
        if start_vel.norm() < 100.0 {
            return Err(SimpleArcError::VelocityTooLow);
        }

        // Assert that both radii are the same length.
        if ((start_loc - center).norm() - (end_loc - center).norm()).abs() >= 1.0 {
            return Err(SimpleArcError::WrongGeometry);
        }

        // Compare the velocity vector to the circle's radius. Since we're starting
        // along a tangent, the angle to the center will be either -90° or 90°.
        let clockwise = start_vel.angle_to(&(start_loc - center)) < 0.0;

        // Go the long way around the circle (more than 180°) if necessary. This avoids
        // an impossible route with discontinuous reversals at each tangent.
        let sweep = (start_loc - center).angle_to(&(end_loc - center));
        let sweep = if clockwise && sweep < 0.0 {
            sweep + 2.0 * PI
        } else if !clockwise && sweep >= 0.0 {
            sweep - 2.0 * PI
        } else {
            sweep
        };

        Ok(Self {
            center,
            radius,
            start_loc,
            start_vel,
            start_boost,
            sweep,
        })
    }

    /// Calculate a rotation of the given angle in this plan's direction.
    fn sweep_by_angle(&self, angle: f32) -> f32 {
        angle * self.sweep.signum()
    }

    /// Calculate the angle between the two points, traveling in this plan's
    /// direction.
    fn sweep_between(&self, start_loc: Point2<f32>, end_loc: Point2<f32>) -> f32 {
        let result = (start_loc - self.center).angle_to(&(end_loc - self.center));
        if result < 0.0 && self.sweep >= 0.0 {
            result + 2.0 * PI
        } else if result > 0.0 && self.sweep < 0.0 {
            result - 2.0 * PI
        } else {
            result
        }
    }

    fn start_rot(&self) -> UnitComplex<f32> {
        let dir = Unit::new_normalize(self.start_vel);
        CAR_LOCAL_FORWARD_AXIS_2D.rotation_to(&dir)
    }

    fn end_loc(&self) -> Point2<f32> {
        self.center + UnitComplex::new(self.sweep) * (self.start_loc - self.center)
    }

    fn end_rot(&self) -> UnitComplex<f32> {
        let dir = Unit::new_normalize(self.end_vel());
        CAR_LOCAL_FORWARD_AXIS_2D.rotation_to(&dir)
    }

    fn end_vel(&self) -> Vector2<f32> {
        UnitComplex::new(self.sweep) * self.start_vel
    }
}

impl SegmentPlan for SimpleArc {
    fn name(&self) -> &str {
        name_of_type!(SimpleArc)
    }

    fn start(&self) -> CarState {
        CarState2D {
            loc: self.start_loc,
            rot: self.start_rot(),
            vel: self.start_vel,
            boost: self.start_boost,
        }
        .to_3d()
    }

    fn end(&self) -> CarState {
        CarState2D {
            loc: self.end_loc(),
            rot: self.end_rot(),
            vel: self.end_vel(),
            boost: self.start_boost,
        }
        .to_3d()
    }

    fn duration(&self) -> f32 {
        self.radius * self.sweep.abs() / self.start_vel.norm()
    }

    fn run(&self) -> Box<dyn SegmentRunner> {
        Box::new(SimpleArcRunner::new(self.clone()))
    }

    fn draw(&self, ctx: &mut Context<'_>) {
        let theta1 = Vector2::x().angle_to(&(self.start_loc - self.center));
        let theta2 = theta1 + self.sweep;
        ctx.eeg.draw(Drawable::Arc(
            self.center,
            self.radius,
            theta1.min(theta2),
            theta1.max(theta2),
            color::YELLOW,
        ));
    }
}

struct SimpleArcRunner {
    plan: SimpleArc,
}

impl SimpleArcRunner {
    fn new(plan: SimpleArc) -> Self {
        Self { plan }
    }

    fn calculate_ahead_loc(&self, loc: Point2<f32>, angle: f32) -> Point2<f32> {
        let center_to_loc = loc - self.plan.center;
        let center_to_ahead = self.plan.sweep_by_angle(angle) * center_to_loc;
        self.plan.center + center_to_ahead.normalize() * self.plan.radius
    }
}

impl SegmentRunner for SimpleArcRunner {
    fn name(&self) -> &str {
        name_of_type!(SimpleArcRunner)
    }

    fn execute_old(&mut self, ctx: &mut Context<'_>) -> SegmentRunAction {
        let me = ctx.me();
        let car_loc = me.Physics.loc_2d();
        let car_forward_axis = me.Physics.forward_axis_2d();

        if !GetToFlatGround::on_flat_ground(ctx.me()) {
            ctx.eeg.log(self.name(), "not on flat ground");
            return SegmentRunAction::Failure;
        }

        // Check if we're finished.
        let swept = self.plan.sweep_between(self.plan.start_loc, car_loc);
        if swept.abs() >= self.plan.sweep.abs() {
            return SegmentRunAction::Success;
        }

        let target_loc = self.calculate_ahead_loc(car_loc, 15.0_f32.to_radians());

        ctx.eeg
            .draw(Drawable::ghost_car_ground(target_loc, me.Physics.rot()));

        let angle = car_forward_axis
            .into_inner()
            .angle_to(&(target_loc - car_loc));
        SegmentRunAction::Yield(common::halfway_house::PlayerInput {
            Throttle: 1.0,
            Steer: angle.max(-1.0).min(1.0),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::{
        integration_tests::{TestRunner, TestScenario},
        routing::{
            segments::SimpleArc,
            test::{route_planner_tester, CookedPlanner},
        },
    };
    use nalgebra::{Point2, Point3, Vector2, Vector3};

    #[test]
    #[ignore = "This is a demo, not a test"]
    fn simple_arc_demo() {
        TestRunner::new()
            .scenario(TestScenario {
                car_loc: Point3::new(1000.0, 0.0, 17.01),
                car_vel: Vector3::new(100.0, 0.0, 0.0),
                ..Default::default()
            })
            .behavior(route_planner_tester(CookedPlanner::new(
                SimpleArc::new(
                    Point2::origin(),
                    1000.0,
                    Point2::new(1000.0, 0.0),
                    Vector2::new(0.0, 100.0),
                    0.0,
                    Point2::new(0.0, 1000.0),
                )
                .ok()
                .unwrap(),
            )))
            .run_for_millis(10_000);
    }
}
