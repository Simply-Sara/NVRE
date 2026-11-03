//! What a static call returns.
//!
//! A `.psc` compile unit is one file: when it writes `Game.GetPlayer()`, the
//! declaration of `GetPlayer` is in `Game.psc`, not here. The compiler still
//! has to know the call's result type, because the type decides the temporary
//! it stores into and whether a comparison needs a `cast`.
//!
//! # Where this table comes from
//!
//! It is *measured from the shipped scripts*, not transcribed from a wiki.
//! Every call in a `.pex` writes its result into a temporary, and the
//! function's own local table declares what type that temporary is. So for
//! every `callstatic` in the corpus, the destination temporary's declared type
//! **is** the return type the Creation Kit worked out:
//!
//! ```text
//! callstatic game GetPlayer ::temp1     and the locals say  ::temp1  actor
//! ```
//!
//! Harvested from all 14,026 scripts in `Skyrim - Misc.bsa`: 3,201 call sites,
//! 183 of them static calls, 129 distinct `class.function` pairs with a single
//! unambiguous observed type. `tools/rettypes.py` reproduces the harvest.
//!
//! Two things this table does **not** claim:
//!
//! - A call that never appears in a shipped script is absent, and its result
//!   type is `var` — the emitter then omits the `cast` rather than inventing
//!   one.
//! - Capitalisation in the harvest is inconsistent (`Actor` and `actor` for the
//!   same function, because different scripts use different declarations).
//!   Lookups are case-insensitive and the table keeps the commonest spelling.

/// `(class, function, return type)`, lowercased, measured from the corpus.
pub const STATIC_RETURNS: &[(&str, &str, &str)] = &[
    ("custom", "countlinks", "Int"),
    ("cwscript", "log", "None"),
    ("debug", "centeroncellandwait", "Float"),
    ("debug", "dbsendplayerposition", "None"),
    ("debug", "messagebox", "None"),
    ("debug", "notification", "None"),
    ("debug", "openuserlog", "Bool"),
    ("debug", "quitgame", "None"),
    ("debug", "sendanimationevent", "None"),
    ("debug", "setfootik", "None"),
    ("debug", "setgodmode", "None"),
    ("debug", "takescreenshot", "None"),
    ("debug", "togglecollisions", "None"),
    ("debug", "togglemenus", "None"),
    ("debug", "trace", "None"),
    ("debug", "tracestack", "None"),
    ("dunyngolglowattracttrig", "directmotesat", "None"),
    ("game", "addachievement", "None"),
    ("game", "addhavokballandsocketconstraint", "Bool"),
    ("game", "addperkpoints", "None"),
    ("game", "advanceskill", "None"),
    ("game", "clearprison", "None"),
    ("game", "cleartempeffects", "None"),
    ("game", "disableplayercontrols", "None"),
    ("game", "enablefasttravel", "None"),
    ("game", "enableplayercontrols", "None"),
    ("game", "fadeoutgame", "None"),
    ("game", "fasttravel", "None"),
    ("game", "findclosestactor", "Actor"),
    ("game", "findclosestactorfromref", "actor"),
    (
        "game",
        "findclosestreferenceofanytypeinlist",
        "ObjectReference",
    ),
    (
        "game",
        "findclosestreferenceofanytypeinlistfromref",
        "ObjectReference",
    ),
    ("game", "findclosestreferenceoftype", "ObjectReference"),
    ("game", "findrandomactor", "Actor"),
    ("game", "findrandomactorfromref", "actor"),
    (
        "game",
        "findrandomreferenceofanytypeinlist",
        "ObjectReference",
    ),
    (
        "game",
        "findrandomreferenceofanytypeinlistfromref",
        "ObjectReference",
    ),
    ("game", "findrandomreferenceoftype", "ObjectReference"),
    ("game", "forcefirstperson", "None"),
    ("game", "forcethirdperson", "None"),
    ("game", "getform", "form"),
    ("game", "getformfromfile", "form"),
    ("game", "getplayer", "actor"),
    ("game", "getplayergrabbedref", "ObjectReference"),
    ("game", "getplayerslastriddenhorse", "actor"),
    ("game", "getsunpositionx", "Float"),
    ("game", "getsunpositiony", "Float"),
    ("game", "getsunpositionz", "Float"),
    ("game", "hidetitlesequencemenu", "None"),
    ("game", "incrementskill", "None"),
    ("game", "incrementskillby", "None"),
    ("game", "incrementstat", "None"),
    ("game", "isactivatecontrolsenabled", "Bool"),
    ("game", "isfasttravelcontrolsenabled", "Bool"),
    ("game", "isfightingcontrolsenabled", "Bool"),
    ("game", "islookingcontrolsenabled", "Bool"),
    ("game", "ismenucontrolsenabled", "Bool"),
    ("game", "ismovementcontrolsenabled", "Bool"),
    ("game", "issneakingcontrolsenabled", "Bool"),
    ("game", "iswordunlocked", "Bool"),
    ("game", "playbink", "None"),
    ("game", "precachechargen", "None"),
    ("game", "querystat", "Int"),
    ("game", "quittomainmenu", "None"),
    ("game", "removehavokconstraints", "Bool"),
    ("game", "requestautosave", "None"),
    ("game", "requestsave", "None"),
    ("game", "sendwerewolftransformation", "None"),
    ("game", "servetime", "None"),
    ("game", "setbeastform", "None"),
    ("game", "sethudcartmode", "None"),
    ("game", "setinchargen", "None"),
    ("game", "setplayeraidriven", "None"),
    ("game", "setplayerreportcrime", "None"),
    ("game", "setsittingrotation", "None"),
    ("game", "setsungazeimagespacemodifier", "None"),
    ("game", "shakecamera", "None"),
    ("game", "shakecontroller", "None"),
    ("game", "showfirstpersongeometry", "None"),
    ("game", "showlimitedracemenu", "None"),
    ("game", "showracemenu", "None"),
    ("game", "showtitlesequencemenu", "None"),
    ("game", "showtrainingmenu", "None"),
    ("game", "starttitlesequence", "None"),
    ("game", "teachword", "None"),
    ("game", "triggerscreenblood", "None"),
    ("game", "unlockword", "None"),
    ("game", "usinggamepad", "Bool"),
    ("imagespacemodifier", "removecrossfade", "None"),
    ("math", "abs", "Float"),
    ("math", "asin", "Float"),
    ("math", "atan", "Float"),
    ("math", "ceiling", "Int"),
    ("math", "cos", "Float"),
    ("math", "floor", "Int"),
    ("math", "pow", "Float"),
    ("math", "sin", "Float"),
    ("math", "sqrt", "Float"),
    ("message", "resethelpmessage", "None"),
    ("rpdefault_math", "atan2", "Float"),
    ("rpdefault_math", "clamp", "Float"),
    ("rpdefault_math", "createeulerfrommatrix", "Float[]"),
    ("rpdefault_math", "createeulerfromquaternion", "Float[]"),
    ("rpdefault_math", "creatematrixfromeuler", "Float[]"),
    ("rpdefault_math", "creatematrixfromquaternion", "Float[]"),
    ("rpdefault_math", "createquaternionfromeuler", "Float[]"),
    ("rpdefault_math", "getoffset3ddata", "Float[]"),
    ("rpdefault_math", "getpi", "Float"),
    ("rpdefault_math", "max", "Float"),
    ("rpdefault_math", "min", "Float"),
    ("rpdefault_math", "multiplyquaternions", "Float[]"),
    ("rpdefault_math", "normalizeangle", "Float"),
    ("rpdefault_utility", "movebehind", "None"),
    ("rpdefault_utility", "moverefsbehind", "None"),
    ("screenshot", "takeshot", "None"),
    ("sound", "setinstancevolume", "None"),
    ("sound", "stopinstance", "None"),
    ("utility", "getcurrentgametime", "Float"),
    ("utility", "getcurrentrealtime", "Float"),
    ("utility", "isinmenumode", "Bool"),
    ("utility", "randomfloat", "Float"),
    ("utility", "randomint", "Int"),
    ("utility", "wait", "None"),
    ("utility", "waitgametime", "None"),
    ("utility", "waitmenumode", "None"),
    ("weather", "findweather", "weather"),
    ("weather", "getcurrentweather", "weather"),
    ("weather", "getskymode", "Int"),
    ("weather", "releaseoverride", "None"),
];

/// What `class.function()` returns, or `None` when the corpus never showed it.
///
/// `class` is matched case-insensitively because the source spells the engine's
/// classes any way it likes (`Game`, `game`, `GAME`) while the `.pex` always
/// carries the lowercase form.
pub fn static_return(class: &str, function: &str) -> Option<&'static str> {
    let c = class.to_ascii_lowercase();
    let f = function.to_ascii_lowercase();
    // `RPDefault_Math` and `Math` are the same class under two names.
    let c = c.strip_prefix("rpdefault_").unwrap_or(&c).to_string();
    STATIC_RETURNS
        .iter()
        .find(|(tc, tf, _)| *tc == c && *tf == f)
        .map(|(_, _, t)| *t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four the emitter actually depends on to get a comparison right.
    #[test]
    fn the_common_ones_are_present() {
        assert_eq!(static_return("Game", "GetPlayer"), Some("actor"));
        assert_eq!(static_return("Utility", "RandomFloat"), Some("Float"));
        assert_eq!(static_return("Utility", "RandomInt"), Some("Int"));
        assert_eq!(static_return("Math", "Abs"), Some("Float"));
    }

    /// Lookup must not care how the source capitalised the class.
    #[test]
    fn class_lookup_ignores_case() {
        assert_eq!(
            static_return("GAME", "getplayer"),
            static_return("game", "GetPlayer")
        );
        assert_eq!(
            static_return("RPDefault_Math", "Abs"),
            static_return("math", "abs")
        );
    }

    /// An unknown call is `None`, never a guess.
    #[test]
    fn unknown_is_none() {
        assert_eq!(static_return("Game", "NoSuchFunctionAnywhere"), None);
    }
}
