use crate::{
    behavior::{
        defense::{retreat::Retreat, retreating_save::RetreatingSave, PanicDefense},
        offense::TepidHit,
        strike::{GroundedHitAimContext, GroundedHitTarget, GroundedHitTargetAdjust},
    },
    eeg::Event,
    helpers::hit_angle::blocking_angle,
    strategy::{Action, Behavior, Context, Game, Scenario},
    utils::{geometry::ExtendF32, WallRayCalculator},
};
use common::prelude::*;
use nalgebra::Vector2;
use nameof::name_of_type;
use simulate::linear_interpolate;
use std::f32::consts::PI;

pub struct Defense;

impl Defense {
    pub fn new() -> Self {
        Self
    }

    pub fn is_between_ball_and_own_goal(
        game: &Game<'_>,
        car: &common::halfway_house::PlayerInfo,
        scenario: &Scenario<'_>,
    ) -> bool {
        let goal = game.own_goal();
        let goal_loc = goal.center_2d;
        let me_loc = car.Physics.loc_2d();
        let me_vel = car.Physics.vel_2d();
        let ball_loc = scenario.ball_prediction().start().loc.to_2d();

        if PanicDefense::finished_panicking(goal, me_loc, me_vel) {
            // Avoid an infinite loop.
            return true;
        }

        // Project our location on a line drawn from the goal to the ball.
        let goal_to_ball_axis = (ball_loc - goal_loc).to_axis();
        let ball_dist = (ball_loc - goal_loc).dot(&goal_to_ball_axis);
        let me_dist = (me_loc - goal_loc).dot(&goal_to_ball_axis);

        // If the play is rapidly moving towards the danger zone and we don't have
        // possession, the danger of a shot is high and if we try to stop it we'll get
        // beat to the ball. Bias towards panicking rather than trying to intercept,
        // this way at least we're between the ball and our goal.
        let panic_factor = if scenario.slightly_panicky_retreat() {
            2000.0
        } else {
            0.0
        };

        if ball_dist <= me_dist + panic_factor {
            return false;
        }

        let defending_angle = (ball_loc - goal_loc).angle(&(me_loc - goal_loc));
        if defending_angle.abs() >= PI / 6.0 {
            // If we're in net, chances are our angle of defense is fine already. e.g. we
            // might be opposite the desired angle, which would be 180° away according to
            // the math, but is a perfectly fine place to be.
            if (me_loc - goal_loc).norm() >= 1200.0 {
                return false;
            }
        }

        true
    }

    pub fn enemy_can_shoot(ctx: &mut Context<'_>) -> bool {
        let (_enemy, intercept) = match ctx.scenario.enemy_intercept() {
            Some(i) => i,
            None => return false,
        };
        let ball_loc = intercept.ball_loc.to_2d();
        let goal = ctx.game.own_goal();
        let dist_ball_to_goal = (ball_loc - goal.center_2d).norm();
        if ctx.scenario.possession() >= -Scenario::POSSESSION_CONTESTABLE {
            return false;
        }
        ctx.enemy_cars().any(|enemy| {
            let angle_car_ball = enemy
                .Physics
                .loc_2d()
                .negated_difference_and_angle_to(ball_loc);
            let angle_ball_goal = ball_loc.negated_difference_and_angle_to(goal.center_2d);
            let angle_diff = (angle_car_ball - angle_ball_goal).normalize_angle().abs();
            let max_angle_diff =
                linear_interpolate(&[2500.0, 7500.0], &[PI / 2.0, PI / 4.0], dist_ball_to_goal);
            angle_diff < max_angle_diff
        })
    }

    pub fn enemy_can_attack(ctx: &mut Context<'_>) -> bool {
        if ctx.scenario.possession() >= -2.0 {
            return false;
        }
        let (enemy, intercept) = match ctx.scenario.enemy_intercept() {
            Some(x) => x,
            None => return false,
        };
        let goal = ctx.game.own_goal();
        let enemy_to_ball = intercept.ball_loc.to_2d() - enemy.Physics.loc_2d();
        let enemy_forward_axis = enemy.Physics.forward_axis_2d();

        enemy_to_ball.angle_to(&-goal.normal_2d).abs() < PI / 3.0
            && enemy_forward_axis.angle_to(&enemy_to_ball).abs() < PI / 3.0
    }
}

impl Behavior for Defense {
    fn name(&self) -> &str {
        name_of_type!(Defense)
    }

    fn execute_old(&mut self, ctx: &mut Context<'_>) -> Action {
        ctx.eeg.track(Event::Defense);

        // If we're not between the ball and our goal, get there.
        if !Self::is_between_ball_and_own_goal(ctx.game, ctx.me(), ctx.scenario) {
            ctx.eeg.log(self.name(), "not between ball and goal");
            return Action::tail_call(Retreat::new());
        }

        // If we need to make a save, do so.
        if RetreatingSave::applicable(ctx).is_ok() {
            ctx.eeg.log(self.name(), "retreating save");
            return Action::tail_call(Retreat::new());
        }

        if Self::enemy_can_shoot(ctx) {
            ctx.eeg.log(self.name(), "enemy_can_shoot");
            return Action::tail_call(Retreat::new());
        }

        if Self::enemy_can_attack(ctx) {
            ctx.eeg.log(self.name(), "enemy_can_attack");
            return Action::tail_call(Retreat::new());
        }

        // If we're already in goal, try to take control of the ball.
        Action::tail_call(TepidHit::new())
    }
}

/// For `GroundedHit::hit_towards`, calculate an aim location which puts us
/// between the ball and our own goal.
pub fn defensive_hit(ctx: &mut GroundedHitAimContext<'_, '_>) -> Result<GroundedHitTarget, ()> {
    let goal_center = ctx.game.own_goal().center_2d;
    let ball_loc = ctx.intercept_ball_loc.to_2d();
    let car_loc = ctx.car.Physics.loc_2d();

    let target_angle = blocking_angle(
        ctx.intercept_ball_loc.to_2d(),
        car_loc,
        goal_center,
        PI / 6.0,
    );
    let aim_loc = ball_loc - Vector2::unit(target_angle) * 4000.0;
    let dist_defense = (goal_center - car_loc).norm();
    let defense_angle = (ball_loc - goal_center).angle_to(&(ball_loc - car_loc));

    let adjust = if dist_defense < 2500.0 && defense_angle.abs() < PI / 3.0 {
        GroundedHitTargetAdjust::StraightOn
    } else {
        GroundedHitTargetAdjust::RoughAim
    };

    let aim_loc = WallRayCalculator::calculate(ball_loc, aim_loc);
    let aim_wall = WallRayCalculator::wall_for_point(ctx.game, aim_loc);
    let dodge = TepidHit::should_dodge(ctx, aim_wall);

    Ok(GroundedHitTarget::new(ctx.intercept_time, adjust, aim_loc).dodge(dodge))
}

#[cfg(test)]
mod integration_tests {
    use crate::{
        behavior::defense::{Defense, HitToOwnCorner},
        eeg::Event,
        integration_tests::{TestRunner, TestScenario},
        strategy::SOCCAR_GOAL_BLUE,
    };
    use brain_test_data::recordings;
    use common::{prelude::*, rl};
    use nalgebra::{Point2, Point3, Rotation3, Vector3};

    #[test]
    fn coming_in_hot_swat_away() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-1004.2267, -1863.0571, 93.15),
                ball_vel: Vector3::new(1196.1945, -1186.7386, 0.0),
                car_loc: Point3::new(1692.9968, -2508.7695, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.009779127, -2.0910075, 0.0),
                car_vel: Vector3::new(-896.0074, -1726.876, 8.375226),
                enemy_loc: Point3::new(1500.0, -4000.0, 17.01),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(2000);

        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.vel());
        assert!(packet.GameBall.Physics.vel().y >= 500.0);
    }

    #[test]
    fn bouncing_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-3143.9788, -241.96017, 1023.1816),
                ball_vel: Vector3::new(717.56323, -1200.3536, 331.91443),
                car_loc: Point3::new(-4009.9998, -465.8022, 86.914),
                car_rot: Rotation3::from_unreal_angles(-0.629795, -0.7865487, 0.5246214),
                car_vel: Vector3::new(982.8443, -1059.1908, -935.80194),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(6000);

        assert!(!test.enemy_has_scored());
    }

    #[test]
    fn redirect_away_from_goal() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2667.985, 779.3049, 186.92154),
                ball_vel: Vector3::new(760.02606, -1394.5569, -368.39642),
                car_loc: Point3::new(-2920.1282, 1346.1251, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.1758921, 0.0),
                car_vel: Vector3::new(688.0767, -1651.0865, 8.181303),
                enemy_loc: Point3::new(-2600.0, 1000.0, 17.01),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(100);

        // This result is just *okay*
        test.examine_events(|events| {
            assert!(events.contains(&Event::Defense));
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromLeftToRight));
            assert!(!events.contains(&Event::PushFromRightToLeft));
        });
    }

    #[test]
    #[ignore = "TODO"]
    fn last_second_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-1150.811, -1606.0569, 102.36157),
                ball_vel: Vector3::new(484.87906, -1624.8169, 32.10115),
                car_loc: Point3::new(-1596.7955, -1039.2034, 17.0),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.4007162, 0.0000958738),
                car_vel: Vector3::new(242.38637, -1733.6719, 8.41),
                boost: 0,
                ..Default::default()
            })
            .soccar()
            .run_for_millis(3000);

        assert!(!test.enemy_has_scored());
    }

    #[test]
    #[ignore = "TODO"]
    fn slow_bouncer() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2849.355, -2856.8281, 1293.4608),
                ball_vel: Vector3::new(907.1093, -600.48956, 267.59674),
                car_loc: Point3::new(1012.88916, -3626.2666, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -0.8467574, 0.0),
                car_vel: Vector3::new(131.446, -188.83897, 8.33),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(3000);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.loc().x < -2000.0);
        assert!(packet.GameBall.Physics.vel().x < -1000.0);
    }

    #[test]
    fn falling_save_from_the_side() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(2353.9868, -5024.7144, 236.38712),
                ball_vel: Vector3::new(-1114.3461, 32.5409, 897.3589),
                car_loc: Point3::new(2907.8083, -4751.0806, 17.010809),
                car_rot: Rotation3::from_unreal_angles(-0.018216021, -2.7451544, -0.0073822825),
                car_vel: Vector3::new(-1412.7858, -672.18933, -6.2963967),
                boost: 0,
                ..Default::default()
            })
            .soccar()
            .run_for_millis(3000);

        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.vel());
        assert!(packet.GameBall.Physics.vel().x < -800.0);
    }

    #[test]
    fn retreating_push_to_corner() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(436.92395, 1428.1085, 93.15),
                ball_vel: Vector3::new(-112.55582, -978.27814, 0.0),
                car_loc: Point3::new(1105.1365, 2072.0022, 17.0),
                car_rot: Rotation3::from_unreal_angles(-0.009491506, -2.061095, -0.0000958738),
                car_vel: Vector3::new(-546.6459, -1095.6816, 8.29),
                enemy_loc: Point3::new(-600.0, 2000.0, 17.01),
                ..Default::default()
            })
            .behavior(Defense::new())
            .run_for_millis(1500);

        test.examine_events(|events| {
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromRightToLeft));
            assert!(!events.contains(&Event::PushFromLeftToRight));
        });

        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.Velocity);
        assert!(packet.GameBall.Physics.vel().norm() >= 1500.0);
    }

    #[test]
    #[ignore = "TODO"]
    fn retreating_push_to_corner_from_awkward_side() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(1948.3385, 1729.5826, 97.89405),
                ball_vel: Vector3::new(185.58005, -1414.3043, -5.051092),
                car_loc: Point3::new(896.22095, 1962.7969, 15.68419),
                car_rot: Rotation3::from_unreal_angles(-0.0131347105, -2.0592732, -0.010450244),
                car_vel: Vector3::new(-660.1856, -1449.2916, -3.7354965),
                ..Default::default()
            })
            .behavior(Defense::new())
            .run_for_millis(2000);

        test.examine_events(|events| {
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromLeftToRight));
            assert!(!events.contains(&Event::PushFromRightToLeft));
        });

        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.Velocity);
        assert!(packet.GameBall.Physics.vel().norm() >= 2000.0);
    }

    #[test]
    fn retreating_push_to_corner_from_awkward_angle() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2365.654, -86.64402, 114.0818),
                ball_vel: Vector3::new(988.47064, -1082.8477, -115.50357),
                car_loc: Point3::new(-2708.0007, -17.896847, 250.98781),
                car_rot: Rotation3::from_unreal_angles(0.28522456, -0.8319928, -0.05263472),
                car_vel: Vector3::new(550.82794, -1164.1539, -277.63806),
                enemy_loc: Point3::new(-2400.0, 100.0, 17.01),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(2000);

        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.Velocity);
        assert!(packet.GameBall.Physics.vel().norm() >= 2000.0);
    }

    #[test]
    #[ignore = "The great bankruptcy of 2018"]
    fn push_from_corner_to_corner() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(1620.9868, -4204.8145, 93.14),
                ball_vel: Vector3::new(-105.58675, 298.33023, 0.0),
                car_loc: Point3::new(3361.587, -4268.589, 16.258373),
                car_rot: Rotation3::from_unreal_angles(-0.0066152923, 1.5453898, -0.005752428),
                car_vel: Vector3::new(89.86856, 1188.811, 7.4339933),
                ..Default::default()
            })
            .behavior(HitToOwnCorner::new())
            .run_for_millis(2000);

        test.examine_events(|events| {
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromRightToLeft));
            assert!(!events.contains(&Event::PushFromLeftToRight));
        });
        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.vel().norm() >= 2000.0);
    }

    #[test]
    fn push_from_corner_to_corner_2() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(2517.809, -4768.475, 93.13),
                ball_vel: Vector3::new(-318.6226, 490.17892, 0.0),
                car_loc: Point3::new(3742.2703, -3277.4558, 16.954643),
                car_rot: Rotation3::from_unreal_angles(-0.009108011, 2.528288, -0.0015339808),
                car_vel: Vector3::new(-462.4023, 288.65112, 9.278907),
                enemy_loc: Point3::new(3000.0, -2500.0, 17.01),
                boost: 10,
                ..Default::default()
            })
            .soccar()
            .run_for_millis(3000);

        assert!(!test.enemy_has_scored());
        test.examine_events(|events| {
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromRightToLeft));
            assert!(!events.contains(&Event::PushFromLeftToRight));
        });
        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.vel().norm() >= 2000.0);
    }

    #[test]
    fn same_side_corner_push() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2545.9438, -4174.64, 318.26862),
                ball_vel: Vector3::new(985.6374, -479.52872, -236.39767),
                car_loc: Point3::new(-1808.3466, -3266.7039, 16.41444),
                car_rot: Rotation3::from_unreal_angles(-0.009203885, -0.65855706, -0.0015339808),
                car_vel: Vector3::new(947.339, -565.98175, 15.669456),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(3500);

        test.examine_events(|events| {
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromRightToLeft));
            assert!(!events.contains(&Event::PushFromLeftToRight));
        });
        assert!(!test.enemy_has_scored());
        // This would be ideal, but it doesn't happen right now:
        // let packet = test.sniff_packet();
        // println!("{:?}", packet.GameBall.Physics.vel());
        // assert!(packet.GameBall.Physics.vel().x < -300.0);
    }

    #[test]
    #[ignore = "I think I need more specific logic for this"]
    fn slow_rolling_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(1455.9731, -4179.0796, 93.15),
                ball_vel: Vector3::new(-474.48724, -247.0518, 0.0),
                car_loc: Point3::new(2522.638, -708.08484, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, 2.6835077, 0.0),
                car_vel: Vector3::new(-1433.151, 800.56586, 8.33),
                boost: 0,
                ..Default::default()
            })
            .soccar()
            .run_for_millis(5000);

        assert!(!test.enemy_has_scored());
        let packet = test.sniff_packet();
        assert!((packet.GameBall.Physics.loc_2d() - SOCCAR_GOAL_BLUE.center_2d).norm() >= 500.0);
    }

    #[test]
    fn slow_retreating_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(1446.3031, -2056.4917, 213.57251),
                ball_vel: Vector3::new(-1024.0333, -1593.1566, -244.15135),
                car_loc: Point3::new(314.3022, -1980.4884, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.7653242, 0.0),
                car_vel: Vector3::new(-268.87683, -1383.9724, 8.309999),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(2000);

        assert!(!test.enemy_has_scored());
        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.loc().x >= 1000.0);
        assert!(packet.GameBall.Physics.vel().x >= 500.0);
    }

    #[test]
    #[ignore = "it's broke, because BounceShot::rough_shooting_spot is getting too complex"]
    fn fast_retreating_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(63.619453, -336.2556, 93.03),
                ball_vel: Vector3::new(-189.17311, -1918.067, 0.0),
                car_loc: Point3::new(-103.64991, 955.411, 16.99),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.5927514, 0.0),
                car_vel: Vector3::new(-57.26778, -2296.9263, 8.53),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(4000);

        assert!(!test.enemy_has_scored());
        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.loc().x < 1000.0);
        assert!(packet.GameBall.Physics.vel().x < 500.0);
    }

    #[test]
    fn jump_save_from_inside_goal() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::JUMP_SAVE_FROM_INSIDE_GOAL, 106.0)
            .starting_boost(0.0)
            .soccar()
            .run_for_millis(3000);
        assert!(!test.enemy_has_scored());
    }

    #[test]
    #[ignore = "The great bankruptcy of 2018"]
    fn retreat_then_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2503.1099, -3172.46, 92.65),
                ball_vel: Vector3::new(796.011, -1343.8209, 0.0),
                car_loc: Point3::new(-3309.3298, -1332.26, 17.01),
                car_rot: Rotation3::from_unreal_angles(0.009505707, -0.79850733, -0.000105084495),
                car_vel: Vector3::new(543.18097, -569.061, 8.321),
                ..Default::default()
            })
            .starting_boost(0.0)
            .soccar()
            .run_for_millis(6000);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.Location.X < -1000.0);
        assert!(!test.enemy_has_scored());
    }

    #[test]
    #[ignore = "The great bankruptcy of 2018"]
    fn clear_around_goal_wall() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::CLEAR_AROUND_GOAL_WALL, 327.0)
            .starting_boost(100.0)
            .soccar()
            .run_for_millis(3000);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.Location.X < -1000.0);
        assert!(packet.GameBall.Physics.Velocity.X < -100.0);
        assert!(!test.enemy_has_scored());
    }

    /// This guards against a behavior where even a tiny touch by the enemy
    /// triggers SameBallTrajectory and causes us to turn around and retreat
    /// back to goal.
    #[test]
    fn defensive_confidence() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::DEFENSIVE_CONFIDENCE, 24.0)
            .starting_boost(65.0)
            .soccar()
            .run_for_millis(3500);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.Velocity.Y >= 500.0);
    }

    #[test]
    fn do_not_own_goal() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(2972.65, -4341.88, 1418.28),
                ball_vel: Vector3::new(-1411.2909, 212.371, 486.57098),
                car_loc: Point3::new(-2043.4099, -1165.84, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.009681773, -0.7725685, 0.00012306236),
                car_vel: Vector3::new(1125.0809, -1248.741, 8.311),
                ..Default::default()
            })
            .starting_boost(10.0)
            .soccar()
            .run_for_millis(4000);

        assert!(!test.enemy_has_scored());
    }

    #[test]
    fn low_boost_block_goal() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::BLOCK_GOAL_WITH_NO_BOOST, 61.5)
            .starting_boost(0.0)
            .enemy_starting_boost(50.0)
            .soccar()
            .run_for_millis(2500);

        assert!(!test.enemy_has_scored());
    }

    #[test]
    fn inconvenient_angle_hit_to_the_side() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::INCONVENIENT_ANGLE_HIT_TO_THE_SIDE, 419.5)
            .starting_boost(0.0)
            .enemy_starting_boost(0.0)
            .soccar()
            .run_for_millis(5000);

        assert!(!test.enemy_has_scored());
    }

    #[test]
    #[ignore = "not working"]
    fn wide_shots_are_not_safe() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::WIDE_SHOTS_ARE_NOT_SAFE, 301.0)
            .starting_boost(12.0)
            .enemy_starting_boost(12.0)
            .soccar()
            .run_for_millis(2500);

        let packet = test.sniff_packet();
        println!("ball loc = {:?}", packet.GameBall.Physics.loc());
        // Push the ball to the corner instead of leaving it
        assert!(packet.GameBall.Physics.loc().x >= 2500.0);
    }

    #[test]
    fn falling_in_front_of_far_corner() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(882.9138, -5002.2944, 608.2664),
                ball_vel: Vector3::new(-211.04604, 37.17434, 459.58438),
                car_loc: Point3::new(-2512.3357, -2450.706, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.009683254, -0.68204623, -0.0000958738),
                car_vel: Vector3::new(786.13666, -620.0981, 8.309999),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(2500);

        assert!(!test.enemy_has_scored());
        let packet = test.sniff_packet();
        let own_goal = Point2::new(0.0, -rl::FIELD_MAX_Y);
        let goal_to_ball_dist = (packet.GameBall.Physics.loc_2d() - own_goal).norm();
        assert!(goal_to_ball_dist >= 750.0);
        assert!(packet.GameBall.Physics.vel().norm() >= 1000.0);
    }

    #[test]
    fn rolling_quickly() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(2792.5564, 2459.176, 94.02834),
                ball_vel: Vector3::new(-467.7808, -2086.822, -88.445175),
                car_loc: Point3::new(3001.808, 3554.98, 16.99),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.7710767, 0.0000958738),
                car_vel: Vector3::new(-379.28546, -1859.9683, 8.41),
                enemy_loc: Point3::new(3301.808, 3554.98, 16.99),
                enemy_rot: Rotation3::from_unreal_angles(-0.00958738, -1.7710767, 0.0000958738),
                enemy_vel: Vector3::new(-379.28546, -1859.9683, 8.41),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(2500);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.vel().x >= -200.0);
    }

    #[test]
    #[ignore = "TODO"]
    fn rolling_around_corner_into_box() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(3042.6016, -4141.044, 180.57321),
                ball_vel: Vector3::new(-1414.86847, -1357.0486, -0.0),
                car_loc: Point3::new(720.54016, 635.665, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.4134674, 0.0),
                car_vel: Vector3::new(256.23804, -1591.1218, 8.3),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(5000);

        assert!(test.has_scored());
    }

    #[test]
    fn low_bouncing_directly_ahead() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-916.57043, -5028.2397, 449.42386),
                ball_vel: Vector3::new(215.22325, 0.07279097, -403.102),
                car_loc: Point3::new(-320.59094, -2705.4436, 17.02),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.6579456, 0.0),
                car_vel: Vector3::new(-85.847946, -990.35706, 8.0),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(3000);

        assert!(!test.enemy_has_scored());
        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.vel());
        assert!(packet.GameBall.Physics.vel().x < -1000.0);
    }

    #[test]
    #[ignore = "TODO"]
    fn high_loft_in_front_of_goal() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2285.6035, -5024.131, 438.6606),
                ball_vel: Vector3::new(751.0301, 16.736507, 811.52356),
                car_loc: Point3::new(-1805.5178, -2341.8872, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -0.4485935, 0.0),
                car_vel: Vector3::new(1141.101, -487.77042, 8.34),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(5000);

        assert!(test.has_scored());
    }

    #[test]
    fn loft_in_front_of_goal_from_the_side() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2288.2634, -4688.248, 93.15),
                ball_vel: Vector3::new(1281.6293, -1659.181, 0.0),
                car_loc: Point3::new(-3077.711, -3389.5276, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -0.95528656, -0.0000958738),
                car_vel: Vector3::new(1027.5283, -1455.2512, 8.3),
                enemy_loc: Point3::new(-1500.0, -4000.0, 17.01),
                ..Default::default()
            })
            .soccar()
            .run_for_millis(4000);

        assert!(!test.enemy_has_scored());
        let packet = test.sniff_packet();
        println!("loc = {:?}", packet.GameBall.Physics.loc());
        assert!(packet.GameBall.Physics.loc().x >= 1000.0);
        println!("vel = {:?}", packet.GameBall.Physics.vel());
        assert!(packet.GameBall.Physics.vel().x >= 750.0);
    }

    #[test]
    fn prepare_for_shot() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::PREPARE_FOR_SHOT, 221.0)
            .starting_boost(50.0)
            .soccar()
            .run_for_millis(4000);

        assert!(!test.enemy_has_scored());
    }

    #[test]
    fn dont_spin_around_in_goal() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::DONT_SPIN_AROUND_IN_GOAL, 259.0)
            .starting_boost(0.0)
            .soccar()
            .run_for_millis(4000);

        assert!(!test.enemy_has_scored());

        let packet = test.sniff_packet();
        let ball_vel = packet.GameBall.Physics.vel();
        println!("ball_vel = {:?}", ball_vel);
        assert!(ball_vel.y >= 1000.0);
    }

    #[test]
    fn turn_around_and_clear() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2666.5999, -5017.36, 243.87),
                ball_vel: Vector3::new(966.53094, -343.081, 266.391),
                ball_ang_vel: Vector3::new(3.24311, 2.42131, -4.42931),
                car_loc: Point3::new(-998.12, -4455.7197, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.009545783, -0.35805213, -0.000065319546),
                car_vel: Vector3::new(1594.091, -598.131, 8.321),
                car_ang_vel: Vector3::new(-0.00040999998, 0.00061, 0.02191),
                ..Default::default()
            })
            .starting_boost(0.0)
            .soccar()
            .run_for_millis(3000);

        assert!(!test.enemy_has_scored());

        let packet = test.sniff_packet();
        let ball_loc = packet.GameBall.Physics.loc();
        println!("ball_loc = {:?}", ball_loc);
        assert!(ball_loc.x < -2500.0);
    }
}
