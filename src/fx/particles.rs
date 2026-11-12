//! Simple pooled square particles in virtual-canvas coordinates.

use macroquad::prelude::*;

use crate::render::View;

const MAX_PARTICLES: usize = 2500;

#[derive(Clone, Copy)]
pub struct Particle {
    pub pos: Vec2,
    pub vel: Vec2,
    /// Downward acceleration, px/s².
    pub gravity: f32,
    /// Fraction of velocity lost per second.
    pub drag: f32,
    pub life: f32,
    pub size: f32,
    pub color: Color,
    age: f32,
}

impl Particle {
    pub fn new(pos: Vec2, vel: Vec2, life: f32, size: f32, color: Color) -> Self {
        Particle {
            pos,
            vel,
            gravity: 0.0,
            drag: 0.0,
            life,
            size,
            color,
            age: 0.0,
        }
    }

    pub fn gravity(mut self, g: f32) -> Self {
        self.gravity = g;
        self
    }

    pub fn drag(mut self, d: f32) -> Self {
        self.drag = d;
        self
    }
}

#[derive(Default)]
pub struct Particles {
    items: Vec<Particle>,
}

impl Particles {
    pub fn spawn(&mut self, p: Particle) {
        if self.items.len() < MAX_PARTICLES {
            self.items.push(p);
        }
    }

    pub fn update(&mut self, dt: f32) {
        let mut i = 0;
        while i < self.items.len() {
            let p = &mut self.items[i];
            p.age += dt;
            if p.age >= p.life {
                self.items.swap_remove(i);
                continue;
            }
            p.vel.y += p.gravity * dt;
            p.vel *= (1.0 - p.drag * dt).max(0.0);
            p.pos += p.vel * dt;
            i += 1;
        }
    }

    pub fn draw(&self, v: &View) {
        for p in &self.items {
            let t = p.age / p.life;
            let size = p.size * (1.0 - 0.7 * t);
            let color = Color { a: p.color.a * (1.0 - t * t), ..p.color };
            v.rect(p.pos.x - size / 2.0, p.pos.y - size / 2.0, size, size, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particles_expire_and_respect_cap() {
        let mut ps = Particles::default();
        for _ in 0..MAX_PARTICLES + 10 {
            ps.spawn(Particle::new(Vec2::ZERO, Vec2::ZERO, 0.1, 4.0, WHITE));
        }
        assert_eq!(ps.items.len(), MAX_PARTICLES);
        ps.update(0.2);
        assert!(ps.items.is_empty());
    }
}
