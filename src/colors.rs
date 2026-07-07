use crate::config_file::get_env_var;

pub fn env_color_disabled() -> bool {
    env_color_disabled_with(get_env_var)
}

pub fn env_color_forced() -> bool {
    env_color_forced_with(get_env_var)
}

fn env_color_disabled_with(get_env_var: impl Fn(&str) -> String) -> bool {
    !get_env_var("NO_COLOR").is_empty() || get_env_var("CLICOLOR") == "0"
}

fn env_color_forced_with(get_env_var: impl Fn(&str) -> String) -> bool {
    !get_env_var("CLICOLOR_FORCE").is_empty() && get_env_var("CLICOLOR_FORCE") != "0"
}

pub fn is_true_color_supported() -> bool {
    let term = get_env_var("TERM");
    let color_term = get_env_var("COLORTERM");

    term.contains("24bit")
        || term.contains("truecolor")
        || color_term.contains("24bit")
        || color_term.contains("truecolor")
}

pub fn is_256_color_supported() -> bool {
    let term = get_env_var("TERM");
    let color_term = get_env_var("COLORTERM");

    is_true_color_supported() || term.contains("256") || color_term.contains("256")
}

#[allow(dead_code)]
pub struct ColorScheme {
    enabled: bool,
    is_256_enabled: bool,
    has_true_color: bool,
}

impl ColorScheme {
    pub fn new(enabled: bool, is_256_enabled: bool, has_true_color: bool) -> Self {
        ColorScheme {
            enabled,
            is_256_enabled,
            has_true_color,
        }
    }

    pub fn bold(&self, t: &str) -> String {
        if !self.enabled {
            return t.to_string();
        }

        nu_ansi_term::Style::new().bold().paint(t).to_string()
    }

    pub fn red(&self, t: &str) -> String {
        if !self.enabled {
            return t.to_string();
        }

        nu_ansi_term::Color::Red.paint(t).to_string()
    }

    pub fn yellow(&self, t: &str) -> String {
        if !self.enabled {
            return t.to_string();
        }

        nu_ansi_term::Color::Yellow.paint(t).to_string()
    }

    pub fn green(&self, t: &str) -> String {
        if !self.enabled {
            return t.to_string();
        }

        nu_ansi_term::Color::Green.paint(t).to_string()
    }

    #[allow(dead_code)]
    pub fn gray(&self, t: &str) -> String {
        if !self.enabled {
            return t.to_string();
        }

        if self.is_256_enabled {
            nu_ansi_term::Color::Fixed(242).paint(t).to_string()
        } else {
            t.to_string()
        }
    }

    pub fn purple(&self, t: &str) -> String {
        if !self.enabled {
            return t.to_string();
        }

        nu_ansi_term::Color::Purple.paint(t).to_string()
    }

    #[allow(dead_code)]
    pub fn blue(&self, t: &str) -> String {
        if !self.enabled {
            return t.to_string();
        }

        nu_ansi_term::Color::Blue.paint(t).to_string()
    }

    pub fn cyan(&self, t: &str) -> String {
        if !self.enabled {
            return t.to_string();
        }

        nu_ansi_term::Color::Cyan.paint(t).to_string()
    }

    pub fn success_icon(&self) -> String {
        self.green("✔")
    }

    pub fn success_icon_with_color(&self, color: nu_ansi_term::Color) -> String {
        if self.enabled {
            return color.paint("✔").to_string();
        }

        "✔".to_string()
    }

    #[allow(dead_code)]
    pub fn warning_icon(&self) -> String {
        self.yellow("!")
    }

    #[allow(dead_code)]
    pub fn failure_icon(&self) -> String {
        self.red("✘")
    }

    #[allow(dead_code)]
    pub fn failure_icon_with_color(&self, color: nu_ansi_term::Color) -> String {
        if self.enabled {
            return color.paint("✘").to_string();
        }

        "✘".to_string()
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    pub struct TestItem {
        name: String,
        no_color_env: String,
        clicolor_env: String,
        clicolor_force_env: String,
        want: bool,
    }

    #[test]
    fn test_env_color_disabled() {
        let tests = vec![
            TestItem {
                name: "pristine env".to_string(),
                no_color_env: "".to_string(),
                clicolor_env: "".to_string(),
                clicolor_force_env: "".to_string(),
                want: false,
            },
            TestItem {
                name: "NO_COLOR enabled".to_string(),
                no_color_env: "1".to_string(),
                clicolor_env: "".to_string(),
                clicolor_force_env: "".to_string(),
                want: true,
            },
            TestItem {
                name: "CLICOLOR disabled".to_string(),
                no_color_env: "".to_string(),
                clicolor_env: "0".to_string(),
                clicolor_force_env: "".to_string(),
                want: true,
            },
            TestItem {
                name: "CLICOLOR enabled".to_string(),
                no_color_env: "".to_string(),
                clicolor_env: "1".to_string(),
                clicolor_force_env: "".to_string(),
                want: false,
            },
            TestItem {
                name: "CLICOLOR_FORCE has no effect".to_string(),
                no_color_env: "".to_string(),
                clicolor_env: "".to_string(),
                clicolor_force_env: "1".to_string(),
                want: false,
            },
        ];

        for t in tests {
            let got = env_color_disabled_with(|key| match key {
                "NO_COLOR" => t.no_color_env.clone(),
                "CLICOLOR" => t.clicolor_env.clone(),
                "CLICOLOR_FORCE" => t.clicolor_force_env.clone(),
                _ => String::new(),
            });
            assert_eq!(got, t.want, "test {}", t.name);
        }
    }

    #[test]
    fn test_env_color_forced() {
        let tests = vec![
            TestItem {
                name: "pristine env".to_string(),
                no_color_env: "".to_string(),
                clicolor_env: "".to_string(),
                clicolor_force_env: "".to_string(),
                want: false,
            },
            TestItem {
                name: "NO_COLOR enabled".to_string(),
                no_color_env: "1".to_string(),
                clicolor_env: "".to_string(),
                clicolor_force_env: "".to_string(),
                want: false,
            },
            TestItem {
                name: "CLICOLOR disabled".to_string(),
                no_color_env: "".to_string(),
                clicolor_env: "0".to_string(),
                clicolor_force_env: "".to_string(),
                want: false,
            },
            TestItem {
                name: "CLICOLOR enabled".to_string(),
                no_color_env: "".to_string(),
                clicolor_env: "1".to_string(),
                clicolor_force_env: "".to_string(),
                want: false,
            },
            TestItem {
                name: "CLICOLOR_FORCE enabled".to_string(),
                no_color_env: "".to_string(),
                clicolor_env: "".to_string(),
                clicolor_force_env: "1".to_string(),
                want: true,
            },
            TestItem {
                name: "CLICOLOR_FORCE disabled".to_string(),
                no_color_env: "".to_string(),
                clicolor_env: "".to_string(),
                clicolor_force_env: "0".to_string(),
                want: false,
            },
        ];

        for t in tests {
            let got = env_color_forced_with(|key| match key {
                "NO_COLOR" => t.no_color_env.clone(),
                "CLICOLOR" => t.clicolor_env.clone(),
                "CLICOLOR_FORCE" => t.clicolor_force_env.clone(),
                _ => String::new(),
            });

            assert_eq!(got, t.want, "test {}", t.name);
        }
    }
}
