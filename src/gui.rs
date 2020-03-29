use ggez::{graphics, Context};

pub fn graph(ctx: &mut Context) {
    graphics::clear(ctx, [0.1, 0.2, 0.3, 1.0].into());
}