//! Player histories for kinetic network construction and reconfiguration.

use crate::actions::{Action, Face};
use crate::scenario::{Change, Pos, Scenario};

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
