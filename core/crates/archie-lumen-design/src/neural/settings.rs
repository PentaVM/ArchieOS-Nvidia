//! The neural sphere's live settings: defaults, limits and how a change is taken (settings.ts). Anything invalid
//! (not finite, wrong type, a badly written colour) keeps the previous value; an unknown name changes nothing.

pub const LINK_KINDS: usize = 7;
pub const LINK_KIND_NAMES: [&str; LINK_KINDS] = [
    "Esqueleto",
    "Axón",
    "Malla",
    "Puente",
    "Contenido parecido",
    "Enlace explícito",
    "Metadatos",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkSetting {
    pub on: bool,
    pub opacity: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub brightness: f64,
    pub node_size: f64,
    pub glow: f64,
    pub holo: f64,
    pub rotation_speed: f64,
    pub pulses: f64,
    pub pulse_speed: f64,
    pub ghosts: bool,
    pub ghost_opacity: f64,
    pub base: bool,
    /// `#rrggbb`, lower case.
    pub background: String,
    pub link_opacity: f64,
    pub link_budget: f64,
    pub links: [LinkSetting; LINK_KINDS],
    pub motion_response: f64,
}

impl Default for Settings {
    fn default() -> Self {
        let link = |on| LinkSetting { on, opacity: 1.0 };
        Settings {
            brightness: 1.0,
            node_size: 1.0,
            glow: 1.0,
            holo: 1.0,
            rotation_speed: 1.0,
            pulses: 70.0,
            pulse_speed: 1.0,
            ghosts: true,
            ghost_opacity: 1.0,
            base: true,
            background: "#01060b".into(),
            link_opacity: 1.0,
            link_budget: 1.0,
            links: [
                link(true),
                link(true),
                link(false),
                link(false),
                link(false),
                link(false),
                link(false),
            ],
            motion_response: 0.18,
        }
    }
}

/// settings.ts `LIMITS`, by the names the original uses.
pub const LIMITS: [(&str, f64, f64); 12] = [
    ("brightness", 0.2, 3.0),
    ("nodeSize", 0.4, 3.0),
    ("glow", 0.0, 3.0),
    ("holo", 0.0, 1.0),
    ("rotationSpeed", 0.0, 4.0),
    ("pulses", 0.0, 70.0),
    ("pulseSpeed", 0.0, 3.0),
    ("ghostOpacity", 0.0, 2.0),
    ("linkOpacity", 0.0, 3.0),
    ("linkBudget", 0.0, 1.0),
    ("links.opacity", 0.0, 3.0),
    ("motion.response", 0.05, 1.0),
];

fn limit(name: &str) -> (f64, f64) {
    LIMITS
        .iter()
        .find(|(n, ..)| *n == name)
        .map(|&(_, lo, hi)| (lo, hi))
        .unwrap_or((f64::MIN, f64::MAX))
}

fn clamp(v: f64, name: &str) -> f64 {
    let (lo, hi) = limit(name);
    hi.min(lo.max(v))
}

impl Settings {
    /// A numeric setting by its original name; false (nothing changes) when unknown or not finite.
    pub fn set_number(&mut self, key: &str, value: f64) -> bool {
        if !value.is_finite() {
            return false;
        }
        let slot = match key {
            "brightness" => &mut self.brightness,
            "nodeSize" => &mut self.node_size,
            "glow" => &mut self.glow,
            "holo" => &mut self.holo,
            "rotationSpeed" => &mut self.rotation_speed,
            "pulses" => &mut self.pulses,
            "pulseSpeed" => &mut self.pulse_speed,
            "ghostOpacity" => &mut self.ghost_opacity,
            "linkOpacity" => &mut self.link_opacity,
            "linkBudget" => &mut self.link_budget,
            "motion.response" => {
                self.motion_response = clamp(value, "motion.response");
                return true;
            }
            _ => return false,
        };
        *slot = clamp(value, key);
        true
    }

    pub fn set_flag(&mut self, key: &str, value: bool) -> bool {
        match key {
            "ghosts" => self.ghosts = value,
            "base" => self.base = value,
            _ => return false,
        }
        true
    }

    /// `#rrggbb` only; stored lower case.
    pub fn set_background(&mut self, hex: &str) -> bool {
        let ok = hex.len() == 7
            && hex.starts_with('#')
            && hex[1..].chars().all(|c| c.is_ascii_hexdigit());
        if ok {
            self.background = hex.to_ascii_lowercase();
        }
        ok
    }

    /// One link kind's switch and opacity; either may be absent.
    pub fn set_link(&mut self, kind: usize, on: Option<bool>, opacity: Option<f64>) -> bool {
        let Some(link) = self.links.get_mut(kind) else {
            return false;
        };
        if let Some(on) = on {
            link.on = on;
        }
        if let Some(o) = opacity.filter(|o| o.is_finite()) {
            link.opacity = clamp(o, "links.opacity");
        }
        true
    }

    /// index.ts `topologyKey`: what the drawn mesh, bridge and content links depend on.
    pub fn topology_key(&self) -> (u8, u64) {
        let mut bits = 0u8;
        for k in 2..LINK_KINDS {
            if self.links[k].on {
                bits |= 1 << k;
            }
        }
        (bits, self.link_budget.to_bits())
    }

    /// scene.ts `treeFactor`: how much of the skeleton (0) and axons (1) is drawn (0 = off).
    pub fn tree_factor(&self) -> [f64; 2] {
        let f = |k: usize| {
            if self.links[k].on {
                self.links[k].opacity * self.link_opacity
            } else {
                0.0
            }
        };
        [f(0), f(1)]
    }
}
