//! Player histories for kinetic network construction and reconfiguration.

use crate::actions::{Action, Face};
use crate::scenario::{Change, Placed, Pos, Scenario};

fn action(tick: u32, pos: Pos, action: Action) -> Change {
    Change {
        tick,
        pos,
        action: Some(action),
        state: None,
        motor: None,
        turn: None,
    }
}

fn use_item(tick: u32, pos: Pos, item: &str, face: Face, hit: [f64; 3], sneaking: bool) -> Change {
    action(
        tick,
        pos,
        Action::Use {
            item: item.into(),
            face,
            hit,
            yaw: 0.,
            pitch: 0.,
            sneaking,
        },
    )
}

/// Exercise both cog sizes as the clicked gear and the held item. After placement,
/// wrench both shaft ends, turn the casing across its axis, then take it off.
pub fn relays(seed: u64) -> Scenario {
    let click = [0, 8, 0];
    let floor = [0, 7, 0];
    let shaft = [0, 9, 0];
    let items = ["create:cogwheel", "create:large_cogwheel"];
    let clicked = items[((seed >> 6) & 1) as usize];
    let held = items[((seed >> 7) & 1) as usize];
    let casing = ["create:andesite_casing", "create:brass_casing"][((seed >> 8) & 1) as usize];
    let hit = if seed & 512 == 0 {
        [0.5, 1., 0.5]
    } else {
        [0.93, 0.625, 0.7]
    };
    let mut events = vec![
        use_item(5, floor, clicked, Face::Up, Face::Up.point(0.5, 0.5), true),
        use_item(10, click, held, Face::Up, hit, false),
        action(20, shaft, Action::Remove),
        use_item(25, click, casing, Face::Up, Face::Up.point(0.5, 0.5), false),
        use_item(
            30,
            click,
            "create:wrench",
            Face::Down,
            Face::Down.point(0.5, 0.5),
            false,
        ),
        use_item(
            35,
            click,
            "create:wrench",
            Face::Up,
            Face::Up.point(0.5, 0.5),
            false,
        ),
        use_item(
            40,
            click,
            "create:wrench",
            Face::East,
            Face::East.point(0.5, 0.5),
            false,
        ),
        use_item(
            45,
            click,
            "create:wrench",
            Face::Up,
            Face::Up.point(0.5, 0.5),
            true,
        ),
    ];
    events.extend([
        use_item(
            55,
            click,
            "create:wooden_bracket",
            Face::Up,
            Face::Up.point(0.5, 0.5),
            false,
        ),
        use_item(
            60,
            click,
            "create:wrench",
            Face::Up,
            Face::Up.point(0.5, 0.5),
            false,
        ),
        action(65, click, Action::Remove),
    ]);
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 140,
        checkpoints: vec![9, 15, 26, 31, 36, 41, 46, 56, 61, 66],
        blocks: vec![crate::scenario::Placed {
            pos: floor,
            state: "minecraft:stone".into(),
            motor: None,
        }],
        events,
    }
}

/// Place gauges or a vertical gearbox with one, two competing, or no eligible shafts.
pub fn instruments(seed: u64) -> Scenario {
    let face = [Face::Up, Face::East, Face::South][(seed / 64 % 3) as usize];
    let pos = [0, 8, 0];
    let offset = face.dir().offset();
    let cell = [offset[0], pos[1] + offset[1], offset[2]];
    let shaft = match face {
        Face::Up => "y",
        Face::East => "x",
        Face::South => "z",
        _ => unreachable!(),
    };
    let item = [
        "create:speedometer",
        "create:stressometer",
        "create:vertical_gearbox",
    ][(seed / 192 % 3) as usize];
    let on_shaft = (seed / 576).is_multiple_of(3);
    let state = if on_shaft {
        format!("create:shaft[axis={shaft}]")
    } else {
        "minecraft:stone".into()
    };
    let mut blocks = vec![crate::scenario::Placed {
        pos,
        state,
        motor: None,
    }];
    if seed / 576 % 3 == 2 {
        for (axis, [dx, dz]) in [("x", [1, 0]), ("z", [0, 1])] {
            blocks.push(crate::scenario::Placed {
                pos: [cell[0] + dx, cell[1], cell[2] + dz],
                state: format!("create:shaft[axis={axis}]"),
                motor: None,
            });
        }
    }
    let hit = face.point(0.5, 0.5);
    let mut placement = use_item(8, pos, item, face, hit, false);
    if let Some(Action::Use { yaw, .. }) = &mut placement.action {
        *yaw = [0., 90., 180., 270.][(seed / 1728 % 4) as usize];
    }
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 90,
        checkpoints: vec![9, 21],
        blocks,
        events: vec![
            placement,
            use_item(
                20,
                cell,
                "create:wrench",
                Face::North,
                Face::North.point(0.5, 0.5),
                false,
            ),
        ],
    }
}

/// Close a four-cog mesh with the sources already turning, separate it again,
/// and restore it after a source has expired. A source at the opposite corner of
/// the square conveys the same sign, while one next to it conveys the reverse.
pub fn competing_sources(seed: u64) -> Scenario {
    let left = [0, 8, 0];
    let opposite = [1, 8, 1];
    let right = [1, 8, 0];
    let join = [0, 8, 1];
    let second = if seed & 64 == 0 { opposite } else { right };
    let above = |[x, y, z]: Pos| [x, y + 1, z];
    let below = |[x, y, z]: Pos| [x, y - 1, z];
    let up = |tick, pos, item, sneaking| {
        use_item(
            tick,
            pos,
            item,
            Face::Up,
            Face::Up.point(0.5, 0.5),
            sneaking,
        )
    };
    let blocks = [left, opposite, right, join]
        .map(|p| crate::scenario::Placed {
            pos: below(p),
            state: "minecraft:stone".into(),
            motor: None,
        })
        .to_vec();
    let mut events = [left, opposite, right]
        .into_iter()
        .enumerate()
        .map(|(i, p)| up(5 + i as u32, below(p), "create:cogwheel", true))
        .collect::<Vec<_>>();
    events.extend([
        up(10, left, "create:hand_crank", false),
        up(11, second, "create:hand_crank", false),
        up(20, above(left), "minecraft:air", false),
        up(23, above(second), "minecraft:air", seed & 128 != 0),
        up(25, below(join), "create:cogwheel", true),
        action(29, right, Action::Remove),
        up(31, above(second), "minecraft:air", seed & 256 != 0),
        up(34, below(right), "create:cogwheel", true),
        action(38, above(left), Action::Remove),
        up(45, above(second), "minecraft:air", false),
    ]);
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 120,
        checkpoints: vec![19, 21, 24, 26, 30, 32, 35, 39, 46, 70],
        blocks,
        events,
    }
}

/// Place a relay while powered, then reconnect it with either side as the
/// input. Both crank supports are also live shaft neighbours.
pub fn switched_shaft(seed: u64) -> Scenario {
    let relay = [0, 8, 0];
    let floor = [0, 7, 0];
    let signal = [0, 9, 0];
    let floor_use = |tick, item, sneaking| {
        use_item(
            tick,
            floor,
            item,
            Face::Up,
            Face::Up.point(0.5, 0.5),
            sneaking,
        )
    };
    let item = if seed & 64 == 0 {
        "create:clutch"
    } else {
        "create:gearshift"
    };
    let mut blocks = vec![Placed {
        pos: floor,
        state: "minecraft:stone".into(),
        motor: None,
    }];
    let mut events = Vec::new();
    for (side, face) in [(-1, Face::West), (1, Face::East)] {
        let shaft = [side, 8, 0];
        blocks.push(Placed {
            pos: shaft,
            state: "create:shaft[axis=x]".into(),
            motor: None,
        });
        events.push(use_item(
            5,
            shaft,
            "create:hand_crank",
            face,
            face.point(0.5, 0.5),
            false,
        ));
    }
    let turn = |tick, x, face, backwards| {
        use_item(
            tick,
            [x, 8, 0],
            "minecraft:air",
            face,
            face.point(0.5, 0.5),
            backwards,
        )
    };
    events.extend([
        action(8, signal, Action::Signal { strength: 15 }),
        floor_use(10, item, false),
        turn(12, -2, Face::West, false),
        action(17, signal, Action::Remove),
        turn(22, -2, Face::West, false),
        action(23, signal, Action::Signal { strength: 15 }),
        action(29, [-2, 8, 0], Action::Remove),
        turn(32, 2, Face::East, seed & 128 != 0),
        action(37, signal, Action::Remove),
        action(43, relay, Action::Remove),
        floor_use(48, item, true),
        action(52, relay, Action::Remove),
        floor_use(56, item, false),
        turn(60, 2, Face::East, false),
    ]);
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 135,
        checkpoints: vec![11, 14, 18, 24, 30, 34, 38, 44, 49, 57, 62, 90],
        blocks,
        events,
    }
}

/// Attempt assisted large-cog placement with the slot blocked, then drive a
/// diagonal small cog from the controller across the rotation-speed limit.
pub fn controller_cog(seed: u64) -> Scenario {
    let controller = [0, 8, 0];
    let gear = [0, 9, 0];
    let support = [-1, 8, 0];
    let crank = [-2, 8, 0];
    let floor = [0, 7, 0];
    let gear_support = [1, 10, -1];
    let blocker = if seed & 64 == 0 {
        (gear, "minecraft:stone")
    } else {
        ([1, 9, 0], "create:cogwheel[axis=z]")
    };
    let up =
        |tick, p, item, sneak| use_item(tick, p, item, Face::Up, Face::Up.point(0.5, 0.5), sneak);
    let fit_gear = |tick| {
        use_item(
            tick,
            controller,
            "create:large_cogwheel",
            Face::North,
            Face::North.point(0.82, 0.35),
            false,
        )
    };
    let turn = |tick| {
        use_item(
            tick,
            crank,
            "minecraft:air",
            Face::West,
            Face::West.point(0.5, 0.5),
            false,
        )
    };
    let blocks = [
        (support, "create:shaft[axis=x]"),
        (floor, "minecraft:stone"),
        (gear_support, "minecraft:stone"),
        blocker,
    ]
    .map(|(pos, state)| Placed {
        pos,
        state: state.into(),
        motor: None,
    })
    .to_vec();
    let events = vec![
        up(6, floor, "create:rotation_speed_controller", false),
        fit_gear(9),
        action(12, blocker.0, Action::Remove),
        fit_gear(14),
        use_item(
            17,
            gear_support,
            "create:cogwheel",
            Face::South,
            Face::South.point(0.5, 0.5),
            true,
        ),
        use_item(
            19,
            support,
            "create:hand_crank",
            Face::West,
            Face::West.point(0.5, 0.5),
            false,
        ),
        turn(22),
        action(28, controller, Action::SpeedSetting { row: 1, value: 128 }),
        turn(33),
        action(38, controller, Action::SpeedSetting { row: 1, value: 129 }),
    ];
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 115,
        checkpoints: vec![10, 15, 21, 25, 30, 40],
        blocks,
        events,
    }
}

/// Creative-mode fixture for the motor board and the speed gauge's dial
/// breakpoints. The gauge and the relay placements use ordinary block items.
pub fn gauge_thresholds(seed: u64) -> Scenario {
    let motor = [-1, 8, 0];
    let shaft = [0, 8, 0];
    let gauge = [1, 8, 0];
    let side_use = |tick, p, item, face: Face, sneak| {
        use_item(tick, p, item, face, face.point(0.5, 0.5), sneak)
    };
    let item = if seed & 64 == 0 {
        "create:speedometer"
    } else {
        "create:stressometer"
    };
    let mut events = vec![
        action(3, motor, Action::GameMode { creative: true }),
        side_use(5, shaft, "create:creative_motor", Face::West, false),
        side_use(8, shaft, item, Face::East, false),
    ];
    let mut checkpoints = vec![9];
    for (index, value) in [1, 29, 30, 31, 99, 100, 101, 256].into_iter().enumerate() {
        let tick = 12 + index as u32 * 5;
        events.push(action(
            tick,
            motor,
            Action::SpeedSetting {
                row: ((seed >> 7) & 1) as i32,
                value,
            },
        ));
        checkpoints.push(tick + 3);
    }
    events.extend([
        action(55, motor, Action::Remove),
        side_use(60, shaft, "create:creative_motor", Face::West, true),
        action(68, gauge, Action::Remove),
    ]);
    checkpoints.extend([57, 63, 69]);
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 145,
        checkpoints,
        blocks: vec![Placed {
            pos: shaft,
            state: "create:shaft[axis=x]".into(),
            motor: None,
        }],
        events,
    }
}

/// Try to branch a running chain across two connection axes, then remove
/// both ends of the first run and rebuild the connection along the other axis.
pub fn chain_junction(seed: u64) -> Scenario {
    let center = [0, 8, 0];
    let crank = [-2, 8, 0];
    let shaft = [-1, 8, 0];
    let side = [0, 8, 1];
    let rear = [0, 8, -1];
    let top = [0, 9, 0];
    let hanger = [0, 9, -1];
    let place = |tick, pos, item, face: Face, sneaking| {
        let mut change = use_item(tick, pos, item, face, face.point(0.5, 0.5), sneaking);
        if let Some(Action::Use { yaw, .. }) = &mut change.action {
            *yaw = 90.;
        }
        change
    };
    let gearshift = "create:adjustable_chain_gearshift";
    let chain = "create:encased_chain_drive";
    let (first, second) = if seed & 64 == 0 {
        (chain, gearshift)
    } else {
        (gearshift, chain)
    };
    let base = [0, 7, 0];
    let mut blocks = [base, [0, 7, 1], [0, 7, -1], hanger]
        .map(|pos| Placed {
            pos,
            state: "minecraft:stone".into(),
            motor: None,
        })
        .to_vec();
    blocks.push(Placed {
        pos: shaft,
        state: "create:shaft[axis=x]".into(),
        motor: None,
    });
    let events = vec![
        place(6, base, first, Face::Up, false),
        place(8, [0, 7, 1], second, Face::Up, true),
        place(10, [0, 7, -1], second, Face::Up, true),
        place(12, shaft, "create:hand_crank", Face::West, false),
        place(17, crank, "minecraft:air", Face::West, false),
        place(20, hanger, second, Face::South, true),
        place(23, crank, "minecraft:air", Face::West, false),
        action(24, [0, 9, 1], Action::Signal { strength: 15 }),
        action(29, side, Action::Remove),
        action(31, rear, Action::Remove),
        place(33, crank, "minecraft:air", Face::West, false),
        action(36, top, Action::Remove),
        place(40, hanger, second, Face::South, true),
        action(44, [0, 9, 1], Action::Remove),
        action(46, center, Action::Remove),
        place(50, base, first, Face::Up, true),
    ];
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 125,
        checkpoints: vec![16, 18, 22, 27, 30, 32, 35, 42, 47, 52],
        blocks,
        events,
    }
}

/// Extend a pole past several shafts, then block that end and try the other
/// side. Case the resulting shaft, remove the casing, then fit brackets.
pub fn shaft_extension(seed: u64) -> Scenario {
    let floor = [0, 7, 0];
    let shaft = [0, 8, 0];
    let tip = [0, 10, 0];
    let blocker = [0, 11, 0];
    let up = |tick, pos, item, sneak| {
        use_item(tick, pos, item, Face::Up, Face::Up.point(0.5, 0.5), sneak)
    };
    let first = if seed & 64 == 0 {
        "create:andesite_casing"
    } else {
        "create:brass_casing"
    };
    let gear = if seed & 128 == 0 {
        "create:cogwheel"
    } else {
        "create:large_cogwheel"
    };
    let mut place_shaft = up(5, floor, "create:shaft", false);
    if let Some(Action::Use { pitch, .. }) = &mut place_shaft.action {
        *pitch = 80.;
    }
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 125,
        checkpoints: vec![6, 11, 16, 21, 26, 31, 36, 41, 47, 52],
        blocks: [floor, blocker]
            .map(|pos| Placed {
                pos,
                state: "minecraft:stone".into(),
                motor: None,
            })
            .to_vec(),
        events: vec![
            place_shaft,
            up(10, shaft, "create:shaft", false),
            up(15, shaft, "create:shaft", false),
            up(20, shaft, gear, false),
            up(25, tip, first, false),
            up(30, tip, "create:wrench", true),
            up(35, tip, "create:metal_bracket", false),
            up(40, tip, "create:wrench", false),
            up(46, tip, "create:wooden_bracket", false),
            action(51, tip, Action::Remove),
        ],
    }
}

/// Install a sequencer over a live vertical input, add a shaft from its face,
/// and remove drive during a program. Turn it flat using the side wrench.
pub fn vertical_sequencer(seed: u64) -> Scenario {
    use crate::program::{Instruction, Kind, Modifier};
    let support = [0, 7, 0];
    let sequencer = [0, 8, 0];
    let arm = [0, 6, 0];
    let crank = [0, 5, 0];
    let signal = [1, 8, 0];
    let up = |tick, p, item| use_item(tick, p, item, Face::Up, Face::Up.point(0.5, 0.5), false);
    let down =
        |tick, p, item| use_item(tick, p, item, Face::Down, Face::Down.point(0.5, 0.5), false);
    let mut shaft = up(14, sequencer, "create:shaft");
    if let Some(Action::Use { pitch, .. }) = &mut shaft.action {
        *pitch = 80.;
    }
    let wrench = |tick, face: Face| {
        use_item(
            tick,
            sequencer,
            "create:wrench",
            face,
            face.point(0.5, 0.5),
            false,
        )
    };
    let instructions = vec![
        Instruction {
            instruction: Kind::TurnAngle,
            modifier: if seed & 64 == 0 {
                Modifier::Forward
            } else {
                Modifier::BackFast
            },
            value: 360,
        },
        Instruction {
            instruction: Kind::End,
            modifier: Modifier::Forward,
            value: -1,
        },
    ];
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 125,
        checkpoints: vec![13, 17, 24, 27, 33, 39, 45, 49],
        blocks: [support, arm]
            .map(|pos| Placed {
                pos,
                state: "create:shaft[axis=y]".into(),
                motor: None,
            })
            .to_vec(),
        events: vec![
            down(6, arm, "create:hand_crank"),
            down(10, crank, "minecraft:air"),
            up(12, support, "create:sequenced_gearshift"),
            shaft,
            down(18, crank, "minecraft:air"),
            action(19, sequencer, Action::Program { instructions }),
            action(21, signal, Action::Signal { strength: 15 }),
            action(25, crank, Action::Remove),
            action(28, signal, Action::Remove),
            wrench(32, Face::North),
            wrench(38, Face::East),
            wrench(44, Face::Up),
            wrench(48, Face::South),
        ],
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn player_casing_registers_both_cog_sizes() {
        for (seed, name) in [(5, "cogwheel"), (69, "large_cogwheel")] {
            let case = super::relays(seed);
            let snapshot = crate::engine::run(&case);
            assert_eq!(
                snapshot.checkpoints[&26].blocks["0,8,0"].state,
                format!(
                    "create:andesite_encased_{name}[axis=y,bottom_shaft=false,top_shaft=false]"
                )
            );
        }
    }

    #[test]
    fn controller_uses_a_player_crank_and_breaks_the_overspeed_gear() {
        let snapshot = crate::engine::run(&super::controller_cog(6));
        let loaded = &snapshot.checkpoints[&30].blocks;
        assert_eq!(
            loaded["-2,8,0"].state,
            "create:hand_crank[facing=west,waterlogged=false]"
        );
        assert_eq!(loaded["1,10,0"].kinetic.as_ref().unwrap().speed, -256.);
        assert_eq!(
            snapshot.checkpoints[&40].blocks["0,9,0"].state,
            "minecraft:air"
        );
    }
}
