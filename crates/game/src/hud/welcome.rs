use super::panels::Ctx;
use super::ui::*;
use super::HudAction;
use bevy::prelude::*;

const W: f32 = 860.0;

const TITLE: &str = "Welcome to your new colony";
const LINES: [&str; 4] = [
    "You and your family found this stream and chose it for your new home.",
    "Before next winter you will need stores of food and sticks, a lodge to live in, and a dam to hold the water.",
    "Select beavers (click one, or drag a box) and right-click a tree, plant or mud bank to set them collecting, or give them a job in the colony (Tab). Buildings are placed from the bar at the bottom.",
    "Water is your safety and your road. Stay close to it, and use dams and canals to spread your territory safely.",
];

pub fn screen(c: &Ctx) -> Vec<VNode> {
    if !c.welcome {
        return Vec::new();
    }
    let mut ui = Ui::default();
    ui.node(Node { width: Val::Px(W), flex_direction: FlexDirection::Column, row_gap: Val::Px(20.0), ..default() }, |u| {
        u.row(8.0, |r| {
            r.bold(TITLE, 30.0, INK);
            r.spacer();
            r.close_button(HudAction::CloseWelcome, true);
        });
        for line in LINES {
            u.text(line, 20.0, INK).style(|n| n.width = Val::Percent(100.0));
        }
        u.row(0.0, |r| {
            r.spacer();
            r.node(Node { padding: UiRect::axes(Val::Px(32.0), Val::Px(11.0)), ..default() }, |b| {
                b.bold("Start", 20.0, ON_ACCENT);
            })
            .skin(Skin::PebblePrimary)
            .action(HudAction::CloseWelcome);
        });
    });
    let mut card = ui.nodes.pop().expect("welcome card");
    panel(&mut card);
    card.node.padding = UiRect::all(Val::Px(36.0));
    let mut screen = VNode::new(Node { position_type: PositionType::Absolute, left: Val::Px(0.0), right: Val::Px(0.0), top: Val::Px(0.0), bottom: Val::Px(0.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() });
    screen.children.push(card);
    vec![screen]
}
