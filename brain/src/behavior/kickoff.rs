use crate::{
    behavior::{defensive_hit, Action, Behavior, Chain, Priority},
    maneuvers::GroundedHit,
    routing::{
        behavior::FollowRoute,
        models::RoutePlanner,
        plan::{ChainedPlanner, GroundIntercept, GroundStraightPlanner},
        StraightMode,
    },
    strategy::Context,
};
use common::prelude::*;
use nalgebra::Point2;

pub struct Kickoff;

impl Kickoff {
    pub fn new() -> Self {
        Kickoff
    }
}

impl Behavior for Kickoff {
    fn name(&self) -> &str {
        stringify!(Kickoff)
    }

    fn execute2(&mut self, ctx: &mut Context) -> Action {
        let approach: Box<RoutePlanner> = if is_diagonal_kickoff(ctx) {
            let target_loc = Point2::new(
                600.0 * ctx.me().Physics.loc().x.signum(),
                1000.0 * ctx.me().Physics.loc().y.signum(),
            );
            let straight = GroundStraightPlanner::new(target_loc, None, 0.0, StraightMode::Asap)
                .allow_dodging(false);
            Box::new(ChainedPlanner::chain(vec![
                Box::new(straight),
                Box::new(GroundIntercept::new()),
            ]))
        } else if is_off_center_kickoff(ctx) {
            let target_loc = Point2::new(
                100.0 * ctx.me().Physics.loc().x.signum(),
                2500.0 * ctx.me().Physics.loc().y.signum(),
            );
            let straight = GroundStraightPlanner::new(target_loc, None, 0.0, StraightMode::Asap)
                .allow_dodging(false);
            Box::new(ChainedPlanner::chain(vec![
                Box::new(straight),
                Box::new(GroundIntercept::new()),
            ]))
        } else {
            Box::new(GroundIntercept::new().allow_dodging(false))
        };

        Action::call(Chain::new(
            Priority::Idle,
            vec![
                Box::new(FollowRoute::new_boxed(approach)),
                Box::new(GroundedHit::hit_towards(defensive_hit)),
            ],
        ))
    }
}

fn is_off_center_kickoff(ctx: &mut Context) -> bool {
    let car_x = ctx.me().Physics.loc().x;
    (car_x.abs() - 256.0).abs() < 50.0
}

fn is_diagonal_kickoff(ctx: &mut Context) -> bool {
    let car_x = ctx.me().Physics.loc().x;
    car_x.abs() >= 1000.0
}

#[cfg(test)]
mod integration_tests {
    use crate::{
        integration_tests::helpers::{TestRunner, TestScenario},
        strategy::Runner2,
    };
    use brain_test_data::recordings;
    use common::prelude::*;
    use nalgebra::{Point3, Rotation3};
    use std::f32::consts::PI;

    #[test]
    fn kickoff_center() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::KICKOFF_CENTER, 107.0)
            .starting_boost(33.0)
            .behavior(Runner2::soccar())
            .run_for_millis(2500);

        let packet = test.sniff_packet();
        let ball = extrapolate_ball(&packet, 10.0);
        assert!(ball.y >= -5000.0);
    }

    #[test]
    fn kickoff_off_center() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                car_loc: Point3::new(-256.0, -3840.0, 17.01).coords,
                ..Default::default()
            })
            .starting_boost(33.0)
            .behavior(Runner2::soccar())
            .run_for_millis(2500);

        let packet = test.sniff_packet();
        let ball = extrapolate_ball(&packet, 3.0);
        assert!(is_scored(ball));
    }

    #[test]
    fn kickoff_diagonal() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                car_loc: Point3::new(-1952.0, -2464.0, 17.01).coords,
                car_rot: Rotation3::from_unreal_angles(0.0, 0.25 * PI, 0.0),
                ..Default::default()
            })
            .starting_boost(33.0)
            .behavior(Runner2::soccar())
            .run_for_millis(2500);

        let packet = test.sniff_packet();
        let ball = extrapolate_ball(&packet, 3.0);
        assert!(is_scored(ball));
    }

    fn extrapolate_ball(packet: &rlbot::ffi::LiveDataPacket, seconds: f32) -> Point3<f32> {
        let ball_loc = packet.GameBall.Physics.loc();
        let ball_vel = packet.GameBall.Physics.vel();
        eprintln!("ball_loc = {:?}", ball_loc);
        eprintln!("ball_vel = {:?}", ball_vel);
        ball_loc + ball_vel * seconds
    }

    fn is_scored(ball: Point3<f32>) -> bool {
        ball.x.abs() < 1000.0 && ball.y >= 5000.0
    }
}