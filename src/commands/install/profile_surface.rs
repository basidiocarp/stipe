use crate::commands::install::InstallProfile;
use crate::commands::tool_registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ManualProfileMember {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) install_hint: &'static str,
}

const LAMELLA_MEMBER: ManualProfileMember = ManualProfileMember {
    name: "lamella",
    description: "agent packaging and install scripts",
    install_hint: "git clone https://github.com/basidiocarp/lamella && cd lamella && ./lamella install",
};

const CAP_MEMBER: ManualProfileMember = ManualProfileMember {
    name: "cap",
    description: "dashboard frontend",
    install_hint: "git clone https://github.com/basidiocarp/cap && cd cap && npm i && npm run dev:all",
};

const PROFILE_SURFACE_ORDER: &[&str] = &[
    "mycelium", "hyphae", "rhizome", "cortina", "lamella", "cap", "canopy", "volva",
];

#[must_use]
pub(crate) fn manual_member(name: &str) -> Option<ManualProfileMember> {
    match name {
        "lamella" => Some(LAMELLA_MEMBER),
        "cap" => Some(CAP_MEMBER),
        _ => None,
    }
}

#[must_use]
pub(crate) fn expected_profile_tools(profile: InstallProfile) -> Vec<String> {
    let managed = tool_registry::specs_for_profile(profile)
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();

    PROFILE_SURFACE_ORDER
        .iter()
        .filter(|name| {
            managed.contains(name)
                || matches!(
                    (profile, **name),
                    (InstallProfile::Standard, "lamella")
                        | (InstallProfile::FullStack, "lamella" | "cap")
                )
        })
        .map(|name| (*name).to_string())
        .collect()
}
