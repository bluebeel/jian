use super::PenFill;

impl PenFill {
    /// The authored switch is separate from paint opacity and color alpha.
    /// Disabled paints remain in the canonical stack for editing and saving.
    pub fn is_enabled(&self) -> bool {
        self.enabled() != Some(false)
    }

    pub fn enabled(&self) -> Option<bool> {
        match self {
            Self::Solid(body) => body.enabled,
            Self::LinearGradient(body) => body.enabled,
            Self::RadialGradient(body) => body.enabled,
            Self::MeshGradient(body) => body.enabled,
            Self::Shader(body) => body.enabled,
            Self::Image(body) => body.enabled,
        }
    }

    pub fn set_enabled(&mut self, enabled: Option<bool>) {
        match self {
            Self::Solid(body) => body.enabled = enabled,
            Self::LinearGradient(body) => body.enabled = enabled,
            Self::RadialGradient(body) => body.enabled = enabled,
            Self::MeshGradient(body) => body.enabled = enabled,
            Self::Shader(body) => body.enabled = enabled,
            Self::Image(body) => body.enabled = enabled,
        }
    }
}
