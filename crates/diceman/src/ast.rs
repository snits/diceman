// ABOUTME: Abstract Syntax Tree types for dice notation expressions.
// ABOUTME: Represents parsed dice expressions like "4d6kh3+5".

use std::fmt;

use crate::error::{Error, Result};

/// A complete dice expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A literal number.
    Number(i64),
    /// A dice roll plan with optional modifiers.
    Roll(RollPlan),
    /// A binary operation (e.g., addition, subtraction).
    BinOp {
        op: Op,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// A parenthesized group.
    Group(Box<Expr>),
}

/// A dice pool: what to roll.
#[derive(Debug, Clone, PartialEq)]
pub struct DicePool {
    /// Number of dice to roll.
    pub count: u32,
    /// Kind of die (numeric N-sided, percentile, fudge, or Marvel d6).
    pub kind: DieKind,
}

/// A dice roll plan: the normalized execution model produced by the parser.
#[derive(Debug, Clone, PartialEq)]
pub struct RollPlan {
    /// The dice pool groups to roll, concatenated in order before scoring.
    ///
    /// Non-empty. More than one group only occurs when every group's kind is
    /// `DieKind::Narrative` (current scope, not domain law — see spec §2.5).
    pools: Vec<DicePool>,
    /// Modifiers applied to the pool before scoring.
    modifiers: Vec<RollModifier>,
    /// How modified dice are converted to a final numeric result.
    scoring: ScoringMode,
    /// Annotations detecting interesting outcomes (descriptive only).
    annotation_rules: Vec<AnnotationRule>,
}

impl RollPlan {
    /// Build a single-pool `RollPlan`, enforcing the shared invariant set via
    /// `validate` — the Marvel Multiverse invariant and the narrative pairing
    /// rules.
    ///
    /// A plan is Marvel exactly when its scoring is `MarvelMultiverse` and its
    /// pool kind is `MarvelD6`; those two must agree. A Marvel plan must roll
    /// exactly 3 dice, may only carry `Edge`/`Trouble` modifiers, and must
    /// carry `annotation_rules` equal to exactly `[AnnotationRule::MarvelFantastic]`.
    /// A non-Marvel plan may not carry `Edge`/`Trouble` modifiers or a
    /// `MarvelFantastic` annotation, but critical-success/failure annotations
    /// remain allowed. `SymbolCancel` scoring with a non-narrative pool, and
    /// `Triumph`/`Despair` annotation rules on a non-narrative plan, are
    /// rejected (`InvalidNarrativeRoll`); a narrative single-group plan is
    /// accepted here and held to the same rules as `new_narrative`.
    pub fn new(
        pool: DicePool,
        modifiers: Vec<RollModifier>,
        scoring: ScoringMode,
        annotation_rules: Vec<AnnotationRule>,
    ) -> Result<Self> {
        let pools = vec![pool];
        Self::validate(&pools, &modifiers, &scoring, &annotation_rules)?;
        Ok(Self {
            pools,
            modifiers,
            scoring,
            annotation_rules,
        })
    }

    /// Build a `RollPlan` over multiple narrative pool groups.
    ///
    /// Rejects an empty group list and any group whose kind is not
    /// `DieKind::Narrative`, then enforces the full narrative invariant set
    /// (`ScoringMode::SymbolCancel` pairing, no modifiers, non-empty groups,
    /// and exactly the `[Triumph, Despair]` annotation rules) via `validate`.
    pub fn new_narrative(
        pools: Vec<DicePool>,
        modifiers: Vec<RollModifier>,
        scoring: ScoringMode,
        annotation_rules: Vec<AnnotationRule>,
    ) -> Result<Self> {
        if pools.is_empty() {
            return Err(Error::InvalidNarrativeRoll(
                "at least one pool group is required".to_string(),
            ));
        }
        for pool in &pools {
            if !matches!(pool.kind, DieKind::Narrative(_)) {
                return Err(Error::InvalidNarrativeRoll(format!(
                    "expected a narrative die kind, found {:?}",
                    pool.kind
                )));
            }
        }

        Self::validate(&pools, &modifiers, &scoring, &annotation_rules)?;
        Ok(Self::new_unchecked_pools(
            pools,
            modifiers,
            scoring,
            annotation_rules,
        ))
    }

    /// Enforce the plan invariants shared by `new` and `new_narrative`.
    ///
    /// `ScoringMode::SymbolCancel` and all-narrative pool groups imply each
    /// other (both directions). Narrative plans carry no modifiers, roll at
    /// least one die per group, and carry exactly the `[Triumph, Despair]`
    /// annotation rules. Non-narrative plans hold a single pool group, reject
    /// Triumph/Despair rules, and satisfy the Marvel Multiverse invariant.
    ///
    /// The `len > 1 => all narrative` constraint is current scope, not domain
    /// law (see spec §2.5): pool union only has meaning where scoring is
    /// pool-wide.
    fn validate(
        pools: &[DicePool],
        modifiers: &[RollModifier],
        scoring: &ScoringMode,
        annotation_rules: &[AnnotationRule],
    ) -> Result<()> {
        if pools
            .iter()
            .any(|pool| matches!(pool.kind, DieKind::Number(0)))
        {
            return Err(Error::InvalidDieKind(0));
        }

        let all_narrative = pools
            .iter()
            .all(|p| matches!(p.kind, DieKind::Narrative(_)));
        let symbol_cancel = matches!(scoring, ScoringMode::SymbolCancel);

        if symbol_cancel != all_narrative {
            return Err(Error::InvalidNarrativeRoll(
                if symbol_cancel {
                    "SymbolCancel scoring requires every pool group to be a narrative die kind"
                } else {
                    "narrative pool groups require SymbolCancel scoring"
                }
                .to_string(),
            ));
        }

        if symbol_cancel {
            if !modifiers.is_empty() {
                return Err(Error::InvalidNarrativeRoll(
                    "narrative rolls do not support modifiers".to_string(),
                ));
            }
            for pool in pools {
                if pool.count == 0 {
                    return Err(Error::InvalidNarrativeRoll(
                        "narrative pool groups must roll at least one die".to_string(),
                    ));
                }
            }
            if !matches!(
                annotation_rules,
                [AnnotationRule::Triumph, AnnotationRule::Despair]
            ) {
                return Err(Error::InvalidNarrativeRoll(
                    "narrative rolls must carry exactly the Triumph and Despair annotation rules"
                        .to_string(),
                ));
            }
            return Ok(());
        }

        if annotation_rules
            .iter()
            .any(|r| matches!(r, AnnotationRule::Triumph | AnnotationRule::Despair))
        {
            return Err(Error::InvalidNarrativeRoll(
                "Triumph/Despair annotations are only supported on narrative rolls".to_string(),
            ));
        }
        if pools.len() > 1 {
            return Err(Error::InvalidNarrativeRoll(
                "only narrative rolls may combine multiple pool groups".to_string(),
            ));
        }

        Self::validate_marvel(&pools[0], modifiers, scoring, annotation_rules)
    }

    /// Enforce the Marvel Multiverse invariant for a single-pool plan.
    ///
    /// `MarvelMultiverse` scoring and a `MarvelD6` pool kind imply each other.
    /// A Marvel plan rolls exactly 3 dice, carries only `Edge`/`Trouble`
    /// modifiers, and carries exactly `[MarvelFantastic]`. A non-Marvel plan
    /// rejects `Edge`/`Trouble` modifiers and the `MarvelFantastic` rule.
    fn validate_marvel(
        pool: &DicePool,
        modifiers: &[RollModifier],
        scoring: &ScoringMode,
        annotation_rules: &[AnnotationRule],
    ) -> Result<()> {
        let marvel_scoring = matches!(scoring, ScoringMode::MarvelMultiverse);
        let marvel_kind = pool.kind == DieKind::MarvelD6;

        if marvel_scoring != marvel_kind {
            return Err(Error::InvalidMarvelRoll(format!(
                "MarvelMultiverse scoring and MarvelD6 pool kind must be used together \
                 (scoring is MarvelMultiverse: {marvel_scoring}, kind is MarvelD6: {marvel_kind})"
            )));
        }

        if marvel_scoring {
            if pool.count != 3 {
                return Err(Error::InvalidMarvelRoll(format!(
                    "expected 3 dice, found {}",
                    pool.count
                )));
            }
            for modifier in modifiers {
                if !matches!(
                    modifier,
                    RollModifier::Edge { .. } | RollModifier::Trouble { .. }
                ) {
                    return Err(Error::InvalidMarvelRoll(format!(
                        "{modifier:?} is not supported on Marvel rolls; only Edge and Trouble"
                    )));
                }
            }
            if !matches!(annotation_rules, [AnnotationRule::MarvelFantastic]) {
                return Err(Error::InvalidMarvelRoll(
                    "Marvel rolls must carry exactly one MarvelFantastic annotation rule"
                        .to_string(),
                ));
            }
        } else {
            for modifier in modifiers {
                if matches!(
                    modifier,
                    RollModifier::Edge { .. } | RollModifier::Trouble { .. }
                ) {
                    return Err(Error::InvalidMarvelRoll(format!(
                        "{modifier:?} is only supported on Marvel rolls"
                    )));
                }
            }
            if annotation_rules.contains(&AnnotationRule::MarvelFantastic) {
                return Err(Error::InvalidMarvelRoll(
                    "MarvelFantastic annotation is only supported on Marvel rolls".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Build a `RollPlan` without validating the Marvel Multiverse invariant.
    ///
    /// Bypasses the checks performed by `new`; callers must guarantee the
    /// plan is already valid (e.g. the parser and roller, which validate
    /// upstream before assembling the plan).
    pub(crate) fn new_unchecked(
        pool: DicePool,
        modifiers: Vec<RollModifier>,
        scoring: ScoringMode,
        annotation_rules: Vec<AnnotationRule>,
    ) -> Self {
        Self {
            pools: vec![pool],
            modifiers,
            scoring,
            annotation_rules,
        }
    }

    /// Build a `RollPlan` over multiple pool groups without validating any
    /// invariant.
    ///
    /// Bypasses the checks performed by `new`/`new_narrative`; callers must
    /// guarantee the plan is already valid (e.g. the parser's narrative arm).
    pub(crate) fn new_unchecked_pools(
        pools: Vec<DicePool>,
        modifiers: Vec<RollModifier>,
        scoring: ScoringMode,
        annotation_rules: Vec<AnnotationRule>,
    ) -> Self {
        Self {
            pools,
            modifiers,
            scoring,
            annotation_rules,
        }
    }

    /// The dice pool groups to roll, in the order their dice are concatenated.
    pub fn pools(&self) -> &[DicePool] {
        &self.pools
    }

    /// Modifiers applied to the pool before scoring.
    pub fn modifiers(&self) -> &[RollModifier] {
        &self.modifiers
    }

    /// How modified dice are converted to a final numeric result.
    pub fn scoring(&self) -> &ScoringMode {
        &self.scoring
    }

    /// Annotations detecting interesting outcomes (descriptive only).
    pub fn annotation_rules(&self) -> &[AnnotationRule] {
        &self.annotation_rules
    }
}

/// A modifier that transforms the dice pool before scoring.
#[derive(Debug, Clone, PartialEq)]
pub enum RollModifier {
    /// Keep the highest N dice.
    KeepHighest(u32),
    /// Keep the lowest N dice.
    KeepLowest(u32),
    /// Drop the highest N dice.
    DropHighest(u32),
    /// Drop the lowest N dice.
    DropLowest(u32),
    /// Reroll dice matching the condition.
    Reroll {
        /// If true, only reroll once per die.
        once: bool,
        /// The condition for reroll (defaults to 1).
        condition: Option<Condition>,
    },
    /// Explode dice matching the condition.
    Explode {
        /// If true, add explosions to same die (compounding/Shadowrun).
        /// If false, create new dice for each explosion (standard/Roll20).
        compounding: bool,
        /// If true, subtract 1 from each explosion roll's added value.
        penetrating: bool,
        /// Maximum explosions per originating die's chain. None = unlimited
        /// (bounded only by the internal runaway guard).
        limit: Option<u32>,
        /// The condition for explosion (defaults to max value).
        condition: Option<Condition>,
    },
    /// Marvel Edge: reroll the lowest-ranked die, keeping the better result by rank.
    Edge {
        /// Number of reroll steps to apply.
        count: u32,
        /// Which die to target for each step.
        policy: EdgePolicy,
    },
    /// Marvel Trouble: reroll the highest-ranked die, keeping the worse result by rank.
    Trouble {
        /// Number of reroll steps to apply.
        count: u32,
    },
}

/// Which die a Marvel Edge step targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EdgePolicy {
    /// Reroll the lowest-ranked die (tie broken by lowest pool index).
    #[default]
    RerollLowest,
    /// Reroll the Marvel die (index 1) until it shows M, then fall back to RerollLowest.
    ChaseFantastic,
}

/// How modified dice become a final numeric outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum ScoringMode {
    /// Sum the values of non-dropped dice.
    Sum,
    /// Count dice matching the condition instead of summing.
    CountSuccesses(Condition),
    /// Concatenate non-dropped die values as digits (e.g., D66: 3, 5 -> 35).
    DigitConcatenate,
    /// Score a Marvel Multiverse 3dMarvel roll.
    MarvelMultiverse,
    /// Merge all narrative symbol faces and cancel opposing symbols into a
    /// `SymbolsOutcome` (Genesys/Star Wars narrative dice).
    SymbolCancel,
}

/// A rule that detects an interesting outcome (descriptive only; no gameplay effect).
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationRule {
    CriticalSuccess(Condition),
    CriticalFailure(Condition),
    MarvelFantastic,
    /// A narrative roll produced at least one Triumph symbol.
    Triumph,
    /// A narrative roll produced at least one Despair symbol.
    Despair,
}

/// The face value a die landed on.
///
/// Numeric dice produce `Numeric`; special faces carrying narrative symbols
/// (the Marvel M, Genesys/Star Wars faces) produce `Symbols`. Which face is
/// special is carried by the face itself, not by pool position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DieFace {
    /// A numeric face (e.g., a d6 showing 4).
    Numeric(i64),
    /// A face bearing narrative symbols. The Marvel M face is `Symbols`
    /// containing `Symbol::Marvel`; a Genesys blank face is an empty pool.
    Symbols(SymbolPool),
}

impl DieFace {
    /// Return the numeric value of a `Numeric` face, or `None` for a symbol face.
    ///
    /// Symbol faces have no intrinsic numeric value; any numeric meaning a
    /// special face carries (the Marvel M contributing 6, or 1 on auto-fail)
    /// is resolved in scoring against pool context, not stored on the face.
    pub fn as_numeric(self) -> Option<i64> {
        match self {
            DieFace::Numeric(n) => Some(n),
            DieFace::Symbols(_) => None,
        }
    }

    /// Return the numeric value of a face that is numeric by construction.
    ///
    /// For call sites where parser validation guarantees a `Numeric` face:
    /// Sum/CountSuccesses/DigitConcatenate scoring, crit checks, and
    /// reroll/explode/keep-drop on numeric die kinds. Panics on a symbol
    /// face, which those sites can never observe.
    pub(crate) fn numeric_value(self) -> i64 {
        match self {
            DieFace::Numeric(n) => n,
            DieFace::Symbols(_) => {
                panic!("numeric_value on a symbol face; this site is numeric by construction")
            }
        }
    }
}

impl fmt::Display for DieFace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DieFace::Numeric(n) => write!(f, "{n}"),
            DieFace::Symbols(pool) => {
                if pool.is_empty() {
                    return write!(f, "-");
                }
                for &s in Symbol::ALL.iter() {
                    for _ in 0..pool.count(s) {
                        write!(f, "{}", s.abbrev())?;
                    }
                }
                Ok(())
            }
        }
    }
}

/// A narrative symbol that can appear on a die face.
///
/// `Success`/`Advantage`/`Triumph`/`Failure`/`Threat`/`Despair` are the
/// Genesys/Star Wars narrative dice symbols; `Light`/`Dark` are the Star
/// Wars Force die symbols; `Marvel` is the Marvel Multiverse M. These are
/// two disjoint systems and pools from them are never merged in gameplay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Symbol {
    Success,
    Advantage,
    Triumph,
    Failure,
    Threat,
    Despair,
    Light,
    Dark,
    Marvel,
}

impl Symbol {
    /// Total number of `Symbol` variants; the size of `SymbolPool`'s count array.
    const COUNT: usize = 9;

    /// All symbol variants, in declaration order.
    const ALL: [Symbol; Self::COUNT] = [
        Symbol::Success,
        Symbol::Advantage,
        Symbol::Triumph,
        Symbol::Failure,
        Symbol::Threat,
        Symbol::Despair,
        Symbol::Light,
        Symbol::Dark,
        Symbol::Marvel,
    ];

    /// The `SymbolPool` count-array position for this symbol.
    ///
    /// Derived from the enum discriminant, which is the single source of
    /// the `Symbol` -> array-index mapping; nothing else assigns positions.
    fn index(self) -> usize {
        self as usize
    }

    /// The abbreviation used when rendering a symbol on a die face.
    fn abbrev(self) -> &'static str {
        match self {
            Symbol::Success => "S",
            Symbol::Advantage => "A",
            Symbol::Triumph => "Tr",
            Symbol::Failure => "F",
            Symbol::Threat => "Th",
            Symbol::Despair => "De",
            Symbol::Light => "L",
            Symbol::Dark => "Dk",
            Symbol::Marvel => "M",
        }
    }
}

// Verifies `ALL` agrees with `index()` at compile time: since `index()` is
// itself derived from declaration order, this checks `ALL` was not
// reordered or left out of sync when `Symbol` was last edited.
const _: () = {
    let mut i = 0;
    while i < Symbol::COUNT {
        assert!(Symbol::ALL[i] as usize == i);
        i += 1;
    }
};

/// A multiset of `Symbol`s on a die face, or aggregated across a pool.
///
/// `Copy`: a fixed-size count array indexed by `Symbol`. A Genesys blank
/// face is `SymbolPool::new()` — a real face that rolled, with nothing on
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SymbolPool {
    counts: [u8; Symbol::COUNT],
}

impl SymbolPool {
    /// An empty pool (no symbols).
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a pool from a list of symbols, counting repeats.
    pub fn of(symbols: &[Symbol]) -> Self {
        let mut pool = Self::new();
        for &s in symbols {
            pool.add(s, 1);
        }
        pool
    }

    /// The count of `s` in this pool.
    pub fn count(&self, s: Symbol) -> u8 {
        self.counts[s.index()]
    }

    /// Add `n` of symbol `s` to this pool, saturating at `u8::MAX`.
    ///
    /// Saturation is a safety net, not a behavior any realistic pool (fewer
    /// than a dozen dice, at most a couple of symbols per face) should reach.
    pub fn add(&mut self, s: Symbol, n: u8) {
        let idx = s.index();
        self.counts[idx] = self.counts[idx].saturating_add(n);
    }

    /// Merge `other`'s counts into this pool, per-symbol, saturating at `u8::MAX`.
    pub fn merge(&mut self, other: &SymbolPool) {
        for i in 0..Symbol::COUNT {
            self.counts[i] = self.counts[i].saturating_add(other.counts[i]);
        }
    }

    /// True if this pool has at least one of `s`.
    pub fn contains(&self, s: Symbol) -> bool {
        self.count(s) != 0
    }

    /// True if this pool has no symbols at all.
    pub fn is_empty(&self) -> bool {
        self.counts.iter().all(|&c| c == 0)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for SymbolPool {
    /// Serializes as a symbol name -> count map, omitting zero counts
    /// (e.g. `{"Success":1,"Advantage":2}`), not the opaque positional
    /// array the derive would emit. The JSON die-face shape is a public
    /// CLI surface.
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;
        for &s in Symbol::ALL.iter() {
            let count = self.count(s);
            if count != 0 {
                map.serialize_entry(&s, &count)?;
            }
        }
        map.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SymbolPool {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct PoolVisitor;

        impl<'de> serde::de::Visitor<'de> for PoolVisitor {
            type Value = SymbolPool;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map of symbol name to count")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut pool = SymbolPool::new();
                while let Some((s, count)) = map.next_entry::<Symbol, u8>()? {
                    pool.add(s, count);
                }
                Ok(pool)
            }
        }

        deserializer.deserialize_map(PoolVisitor)
    }
}

/// The final outcome of scoring a roll.
///
/// `Numeric` covers summed and digit-concatenated results;
/// `Successes` covers success-counting results;
/// `Marvel` covers a Marvel Multiverse 3dMarvel roll;
/// `Symbols` covers a narrative (Genesys/Star Wars) symbol-cancellation roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RollOutcome {
    Numeric(i64),
    Successes(i64),
    Marvel(MarvelOutcome),
    Symbols(SymbolsOutcome),
}

/// The scored outcome of a narrative (Genesys/Star Wars) symbol roll.
///
/// `successes` and `advantages` are signed nets: each Triumph adds a Success
/// and each Despair adds a Failure to the success net, while the Triumph and
/// Despair counts are reported uncancelled. Light and Dark are Force pips that
/// never cancel and never enter the nets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SymbolsOutcome {
    /// Net success: `(Success + Triumph) - (Failure + Despair)`; negative is net failure.
    pub successes: i64,
    /// Net advantage: `Advantage - Threat`; negative is net threat.
    pub advantages: i64,
    /// Number of Triumph symbols, reported regardless of cancellation.
    pub triumphs: u8,
    /// Number of Despair symbols, reported regardless of cancellation.
    pub despairs: u8,
    /// Number of Light Force pips.
    pub light: u8,
    /// Number of Dark Force pips.
    pub dark: u8,
}

/// The scored outcome of a Marvel Multiverse 3dMarvel roll.
///
/// The Marvel die is the middle die (index 1); its 1 face is M.
/// M counts as 6 for the total except on raw `1 / M / 1`, where M reverts
/// to 1 and the check auto-fails regardless of target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarvelOutcome {
    /// The 3dMarvel total, in 3..=18.
    pub total: i64,
    /// True when the raw roll was `1 / M / 1` (auto-fail regardless of target).
    pub auto_fail: bool,
    /// True when the middle die showed M (its face was 1).
    pub m_shown: bool,
}

impl RollOutcome {
    /// Return the numeric value of the outcome, if it has one.
    ///
    /// `Some` for variants with a numeric value: `Successes` yields the
    /// count, `Marvel` yields the total (lenient extraction consistent with
    /// arithmetic coercion). `Symbols` returns `None`: a narrative outcome has
    /// no single arithmetic value, so it cannot take part in arithmetic.
    pub fn as_numeric(self) -> Option<i64> {
        match self {
            RollOutcome::Numeric(n) | RollOutcome::Successes(n) => Some(n),
            RollOutcome::Marvel(o) => Some(o.total),
            RollOutcome::Symbols(_) => None,
        }
    }
}

/// The target-applied Marvel verdict when the Marvel die shows M.
///
/// `Success` means the check met its target despite M; `Failure` means it
/// missed. Only produced when the middle die showed M (otherwise the roll
/// is not Fantastic and no `Fantastic` value is attached).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Fantastic {
    /// The check succeeded (met or beat the target after modifier, no auto-fail).
    Success,
    /// The check failed (missed the target, or auto-fail overrode an otherwise-passing total).
    Failure,
}

/// The target-applied result of a single Marvel Multiverse 3dMarvel roll.
///
/// Carries the scored `MarvelOutcome` alongside a target/modifier pair and the
/// derived `success`/`fantastic` booleans, plus the formatted roll expression
/// so callers can render the roll without re-running the pipeline.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MarvelCheck {
    /// The scored Marvel outcome (total, auto_fail, m_shown).
    pub outcome: MarvelOutcome,
    /// The target value the check is compared against.
    pub target: i64,
    /// The modifier added to the total before comparison.
    pub modifier: i64,
    /// Whether the check succeeded: `(total + modifier >= target) && !auto_fail`.
    pub success: bool,
    /// The Fantastic verdict when the Marvel die showed M, else `None`.
    pub fantastic: Option<Fantastic>,
    /// The formatted roll expression (e.g., `3dMarvel[3, 3, 4] = 10`).
    pub expression: String,
}

/// A pool-level annotation describing an interesting outcome (descriptive only).
///
/// `Fantastic` and `AutoFail` are populated by Marvel Multiverse scoring
/// (middle die showed M, or raw `1 / M / 1`). Per-die crit rules surface via
/// `DieResult::is_crit_success` and `DieResult::is_crit_failure`, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Annotation {
    /// The Marvel middle die showed M.
    Fantastic,
    /// The Marvel roll was raw `1 / M / 1` (auto-fail regardless of target).
    AutoFail,
    /// A narrative roll produced at least one Triumph symbol.
    Triumph,
    /// A narrative roll produced at least one Despair symbol.
    Despair,
}

/// The type of dice to roll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DieKind {
    /// A die with N sides (d6, d20, etc.).
    Number(u32),
    /// A percentile die (d% = d100).
    Percent,
    /// A fudge die (dF = {-1, 0, 1}).
    Fudge,
    /// The six-sided Marvel Multiverse die used in a 3dMarvel pool.
    MarvelD6,
    /// One of the seven Genesys/Star Wars narrative dice.
    Narrative(NarrativeDie),
}

impl DieKind {
    /// Returns the number of sides for this die type.
    pub fn count(&self) -> u32 {
        match self {
            DieKind::Number(n) => *n,
            DieKind::Percent => 100,
            DieKind::Fudge => 3, // -1, 0, 1
            DieKind::MarvelD6 => 6,
            DieKind::Narrative(die) => die.count(),
        }
    }
}

/// One of the seven Genesys/Star Wars narrative dice.
///
/// Genesys checks use Boost/Setback/Ability/Difficulty/Proficiency/Challenge;
/// Star Wars adds Force. Genesys has no Force die, but diceman does not
/// enforce that distinction — callers choosing which seven dice to expose is
/// a parser/CLI concern, not a type-level one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NarrativeDie {
    Boost,
    Setback,
    Ability,
    Difficulty,
    Proficiency,
    Challenge,
    Force,
}

impl NarrativeDie {
    /// Returns the number of sides (faces) for this narrative die.
    pub fn count(self) -> u32 {
        match self {
            NarrativeDie::Boost | NarrativeDie::Setback => 6,
            NarrativeDie::Ability | NarrativeDie::Difficulty => 8,
            NarrativeDie::Proficiency | NarrativeDie::Challenge | NarrativeDie::Force => 12,
        }
    }

    /// The symbol pool shown on a given roll (1..=`count()`), verified
    /// against Appendix A of the Phase 9 design doc.
    pub(crate) fn face(self, roll: u32) -> SymbolPool {
        use Symbol::*;
        let symbols: &[Symbol] = match self {
            NarrativeDie::Boost => match roll {
                1 | 2 => &[],
                3 => &[Success],
                4 => &[Success, Advantage],
                5 => &[Advantage, Advantage],
                6 => &[Advantage],
                _ => unreachable!("Boost roll out of range: {roll}"),
            },
            NarrativeDie::Setback => match roll {
                1 | 2 => &[],
                3 | 4 => &[Failure],
                5 | 6 => &[Threat],
                _ => unreachable!("Setback roll out of range: {roll}"),
            },
            NarrativeDie::Ability => match roll {
                1 => &[],
                2 | 3 => &[Success],
                4 => &[Success, Success],
                5 | 6 => &[Advantage],
                7 => &[Success, Advantage],
                8 => &[Advantage, Advantage],
                _ => unreachable!("Ability roll out of range: {roll}"),
            },
            NarrativeDie::Difficulty => match roll {
                1 => &[],
                2 => &[Failure],
                3 => &[Failure, Failure],
                4..=6 => &[Threat],
                7 => &[Threat, Threat],
                8 => &[Failure, Threat],
                _ => unreachable!("Difficulty roll out of range: {roll}"),
            },
            NarrativeDie::Proficiency => match roll {
                1 => &[],
                2 | 3 => &[Success],
                4 | 5 => &[Success, Success],
                6 => &[Advantage],
                7..=9 => &[Success, Advantage],
                10 | 11 => &[Advantage, Advantage],
                12 => &[Triumph],
                _ => unreachable!("Proficiency roll out of range: {roll}"),
            },
            NarrativeDie::Challenge => match roll {
                1 => &[],
                2 | 3 => &[Failure],
                4 | 5 => &[Failure, Failure],
                6 | 7 => &[Threat],
                8 | 9 => &[Failure, Threat],
                10 | 11 => &[Threat, Threat],
                12 => &[Despair],
                _ => unreachable!("Challenge roll out of range: {roll}"),
            },
            NarrativeDie::Force => match roll {
                1..=6 => &[Dark],
                7 => &[Dark, Dark],
                8 | 9 => &[Light],
                10..=12 => &[Light, Light],
                _ => unreachable!("Force roll out of range: {roll}"),
            },
        };
        SymbolPool::of(symbols)
    }
}

impl fmt::Display for NarrativeDie {
    /// The full-word die name used in narrative notation (`Ability`, `Force`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            NarrativeDie::Boost => "Boost",
            NarrativeDie::Setback => "Setback",
            NarrativeDie::Ability => "Ability",
            NarrativeDie::Difficulty => "Difficulty",
            NarrativeDie::Proficiency => "Proficiency",
            NarrativeDie::Challenge => "Challenge",
            NarrativeDie::Force => "Force",
        };
        f.write_str(name)
    }
}

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::Add => write!(f, "+"),
            Op::Sub => write!(f, "-"),
            Op::Mul => write!(f, "*"),
            Op::Div => write!(f, "/"),
        }
    }
}

/// A comparison condition for explode/reroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Condition {
    pub compare: Compare,
    pub value: i64,
}

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    Equal,
    NotEqual,
    LessThan,
    LessOrEqual,
    GreaterThan,
    GreaterOrEqual,
}

impl Compare {
    /// Check if the given value satisfies this comparison.
    pub fn check(&self, roll: i64, target: i64) -> bool {
        match self {
            Compare::Equal => roll == target,
            Compare::NotEqual => roll != target,
            Compare::LessThan => roll < target,
            Compare::LessOrEqual => roll <= target,
            Compare::GreaterThan => roll > target,
            Compare::GreaterOrEqual => roll >= target,
        }
    }
}

impl fmt::Display for Compare {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Compare::Equal => write!(f, "="),
            Compare::NotEqual => write!(f, "<>"),
            Compare::LessThan => write!(f, "<"),
            Compare::LessOrEqual => write!(f, "<="),
            Compare::GreaterThan => write!(f, ">"),
            Compare::GreaterOrEqual => write!(f, ">="),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roll_plan_has_annotation_rules() {
        let plan = RollPlan {
            pools: vec![DicePool {
                count: 1,
                kind: DieKind::Number(20),
            }],
            modifiers: vec![],
            scoring: ScoringMode::Sum,
            annotation_rules: vec![
                AnnotationRule::CriticalSuccess(Condition {
                    compare: Compare::Equal,
                    value: 20,
                }),
                AnnotationRule::CriticalFailure(Condition {
                    compare: Compare::Equal,
                    value: 1,
                }),
            ],
        };
        assert!(plan
            .annotation_rules
            .iter()
            .any(|r| matches!(r, AnnotationRule::CriticalSuccess(_))));
        assert!(plan
            .annotation_rules
            .iter()
            .any(|r| matches!(r, AnnotationRule::CriticalFailure(_))));
    }

    #[test]
    fn new_narrative_rejects_empty_pool_list() {
        let result = RollPlan::new_narrative(vec![], vec![], ScoringMode::Sum, vec![]);
        assert!(matches!(result, Err(Error::InvalidNarrativeRoll(_))));
    }

    fn narrative_pool(count: u32, die: NarrativeDie) -> DicePool {
        DicePool {
            count,
            kind: DieKind::Narrative(die),
        }
    }

    fn narrative_rules() -> Vec<AnnotationRule> {
        vec![AnnotationRule::Triumph, AnnotationRule::Despair]
    }

    #[test]
    fn new_narrative_accepts_valid_plan() {
        let result = RollPlan::new_narrative(
            vec![
                narrative_pool(2, NarrativeDie::Ability),
                narrative_pool(1, NarrativeDie::Difficulty),
            ],
            vec![],
            ScoringMode::SymbolCancel,
            narrative_rules(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn new_accepts_single_narrative_group() {
        let result = RollPlan::new(
            narrative_pool(1, NarrativeDie::Force),
            vec![],
            ScoringMode::SymbolCancel,
            narrative_rules(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn new_rejects_symbol_cancel_with_numeric_group() {
        let result = RollPlan::new(
            numeric_pool(),
            vec![],
            ScoringMode::SymbolCancel,
            narrative_rules(),
        );
        assert!(matches!(result, Err(Error::InvalidNarrativeRoll(_))));
    }

    #[test]
    fn new_narrative_rejects_non_symbol_cancel_scoring() {
        let result = RollPlan::new_narrative(
            vec![narrative_pool(1, NarrativeDie::Ability)],
            vec![],
            ScoringMode::Sum,
            narrative_rules(),
        );
        assert!(matches!(result, Err(Error::InvalidNarrativeRoll(_))));
    }

    #[test]
    fn new_narrative_rejects_modifiers() {
        let result = RollPlan::new_narrative(
            vec![narrative_pool(1, NarrativeDie::Ability)],
            vec![RollModifier::KeepHighest(1)],
            ScoringMode::SymbolCancel,
            narrative_rules(),
        );
        assert!(matches!(result, Err(Error::InvalidNarrativeRoll(_))));
    }

    #[test]
    fn new_narrative_rejects_zero_count_group() {
        let result = RollPlan::new_narrative(
            vec![narrative_pool(0, NarrativeDie::Ability)],
            vec![],
            ScoringMode::SymbolCancel,
            narrative_rules(),
        );
        assert!(matches!(result, Err(Error::InvalidNarrativeRoll(_))));
    }

    #[test]
    fn new_narrative_rejects_missing_annotation_rules() {
        let result = RollPlan::new_narrative(
            vec![narrative_pool(1, NarrativeDie::Ability)],
            vec![],
            ScoringMode::SymbolCancel,
            vec![AnnotationRule::Triumph],
        );
        assert!(matches!(result, Err(Error::InvalidNarrativeRoll(_))));
    }

    #[test]
    fn new_narrative_rejects_extra_annotation_rules() {
        let result = RollPlan::new_narrative(
            vec![narrative_pool(1, NarrativeDie::Ability)],
            vec![],
            ScoringMode::SymbolCancel,
            vec![
                AnnotationRule::Triumph,
                AnnotationRule::Despair,
                AnnotationRule::MarvelFantastic,
            ],
        );
        assert!(matches!(result, Err(Error::InvalidNarrativeRoll(_))));
    }

    #[test]
    fn new_rejects_numeric_plan_with_triumph_rule() {
        let result = RollPlan::new(
            numeric_pool(),
            vec![],
            ScoringMode::Sum,
            vec![AnnotationRule::Triumph],
        );
        assert!(matches!(result, Err(Error::InvalidNarrativeRoll(_))));
    }

    #[test]
    fn marvel_d6_has_six_faces() {
        assert_eq!(DieKind::MarvelD6.count(), 6);
    }

    #[test]
    fn narrative_die_counts() {
        assert_eq!(DieKind::Narrative(NarrativeDie::Boost).count(), 6);
        assert_eq!(DieKind::Narrative(NarrativeDie::Setback).count(), 6);
        assert_eq!(DieKind::Narrative(NarrativeDie::Ability).count(), 8);
        assert_eq!(DieKind::Narrative(NarrativeDie::Difficulty).count(), 8);
        assert_eq!(DieKind::Narrative(NarrativeDie::Proficiency).count(), 12);
        assert_eq!(DieKind::Narrative(NarrativeDie::Challenge).count(), 12);
        assert_eq!(DieKind::Narrative(NarrativeDie::Force).count(), 12);
    }

    /// Asserts every roll 1..=expected.len() on `die` produces the pool built
    /// from `expected[roll - 1]`, per Appendix A of the Phase 9 design doc.
    fn assert_die_faces(die: NarrativeDie, expected: &[&[Symbol]]) {
        assert_eq!(die.count() as usize, expected.len(), "{die:?} face count");
        for (i, symbols) in expected.iter().enumerate() {
            let roll = (i + 1) as u32;
            assert_eq!(
                die.face(roll),
                SymbolPool::of(symbols),
                "{die:?} roll {roll}"
            );
        }
    }

    #[test]
    fn boost_die_faces() {
        // blank, blank, S, S+A, A+A, A
        assert_die_faces(
            NarrativeDie::Boost,
            &[
                &[],
                &[],
                &[Symbol::Success],
                &[Symbol::Success, Symbol::Advantage],
                &[Symbol::Advantage, Symbol::Advantage],
                &[Symbol::Advantage],
            ],
        );
    }

    #[test]
    fn setback_die_faces() {
        // blank, blank, F, F, T, T
        assert_die_faces(
            NarrativeDie::Setback,
            &[
                &[],
                &[],
                &[Symbol::Failure],
                &[Symbol::Failure],
                &[Symbol::Threat],
                &[Symbol::Threat],
            ],
        );
    }

    #[test]
    fn ability_die_faces() {
        // blank, S, S, S+S, A, A, S+A, A+A
        assert_die_faces(
            NarrativeDie::Ability,
            &[
                &[],
                &[Symbol::Success],
                &[Symbol::Success],
                &[Symbol::Success, Symbol::Success],
                &[Symbol::Advantage],
                &[Symbol::Advantage],
                &[Symbol::Success, Symbol::Advantage],
                &[Symbol::Advantage, Symbol::Advantage],
            ],
        );
    }

    #[test]
    fn difficulty_die_faces() {
        // blank, F, F+F, T, T, T, T+T, F+T
        assert_die_faces(
            NarrativeDie::Difficulty,
            &[
                &[],
                &[Symbol::Failure],
                &[Symbol::Failure, Symbol::Failure],
                &[Symbol::Threat],
                &[Symbol::Threat],
                &[Symbol::Threat],
                &[Symbol::Threat, Symbol::Threat],
                &[Symbol::Failure, Symbol::Threat],
            ],
        );
    }

    #[test]
    fn proficiency_die_faces() {
        // blank, S, S, S+S, S+S, A, S+A, S+A, S+A, A+A, A+A, TR
        assert_die_faces(
            NarrativeDie::Proficiency,
            &[
                &[],
                &[Symbol::Success],
                &[Symbol::Success],
                &[Symbol::Success, Symbol::Success],
                &[Symbol::Success, Symbol::Success],
                &[Symbol::Advantage],
                &[Symbol::Success, Symbol::Advantage],
                &[Symbol::Success, Symbol::Advantage],
                &[Symbol::Success, Symbol::Advantage],
                &[Symbol::Advantage, Symbol::Advantage],
                &[Symbol::Advantage, Symbol::Advantage],
                &[Symbol::Triumph],
            ],
        );
    }

    #[test]
    fn challenge_die_faces() {
        // blank, F, F, F+F, F+F, T, T, F+T, F+T, T+T, T+T, D
        assert_die_faces(
            NarrativeDie::Challenge,
            &[
                &[],
                &[Symbol::Failure],
                &[Symbol::Failure],
                &[Symbol::Failure, Symbol::Failure],
                &[Symbol::Failure, Symbol::Failure],
                &[Symbol::Threat],
                &[Symbol::Threat],
                &[Symbol::Failure, Symbol::Threat],
                &[Symbol::Failure, Symbol::Threat],
                &[Symbol::Threat, Symbol::Threat],
                &[Symbol::Threat, Symbol::Threat],
                &[Symbol::Despair],
            ],
        );
    }

    #[test]
    fn force_die_faces() {
        // K, K, K, K, K, K, K+K, L, L, L+L, L+L, L+L (no blanks)
        assert_die_faces(
            NarrativeDie::Force,
            &[
                &[Symbol::Dark],
                &[Symbol::Dark],
                &[Symbol::Dark],
                &[Symbol::Dark],
                &[Symbol::Dark],
                &[Symbol::Dark],
                &[Symbol::Dark, Symbol::Dark],
                &[Symbol::Light],
                &[Symbol::Light],
                &[Symbol::Light, Symbol::Light],
                &[Symbol::Light, Symbol::Light],
                &[Symbol::Light, Symbol::Light],
            ],
        );
    }

    #[test]
    fn force_die_has_no_blank_faces() {
        for roll in 1..=NarrativeDie::Force.count() {
            assert!(
                !NarrativeDie::Force.face(roll).is_empty(),
                "Force roll {roll} should not be blank"
            );
        }
    }

    fn marvel_pool() -> DicePool {
        DicePool {
            count: 3,
            kind: DieKind::MarvelD6,
        }
    }

    fn numeric_pool() -> DicePool {
        DicePool {
            count: 1,
            kind: DieKind::Number(20),
        }
    }

    #[test]
    fn new_rejects_zero_sided_numeric_die() {
        let result = RollPlan::new(
            DicePool {
                count: 1,
                kind: DieKind::Number(0),
            },
            vec![],
            ScoringMode::Sum,
            vec![],
        );
        assert!(matches!(result, Err(Error::InvalidDieKind(0))));
    }

    #[test]
    fn new_rejects_marvel_kind_with_non_marvel_scoring() {
        let result = RollPlan::new(marvel_pool(), vec![], ScoringMode::Sum, vec![]);
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_scoring_with_non_marvel_kind() {
        let result = RollPlan::new(
            numeric_pool(),
            vec![],
            ScoringMode::MarvelMultiverse,
            vec![],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_plan_with_wrong_pool_count() {
        let pool = DicePool {
            count: 5,
            kind: DieKind::MarvelD6,
        };
        let result = RollPlan::new(
            pool,
            vec![],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_plan_with_non_edge_trouble_modifier() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![RollModifier::KeepHighest(1)],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_plan_with_empty_annotation_rules() {
        let result = RollPlan::new(marvel_pool(), vec![], ScoringMode::MarvelMultiverse, vec![]);
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_plan_with_critical_annotation_only() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::CriticalSuccess(Condition {
                compare: Compare::Equal,
                value: 6,
            })],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_marvel_plan_with_critical_annotation_alongside_fantastic() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![],
            ScoringMode::MarvelMultiverse,
            vec![
                AnnotationRule::MarvelFantastic,
                AnnotationRule::CriticalSuccess(Condition {
                    compare: Compare::Equal,
                    value: 6,
                }),
            ],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_non_marvel_plan_with_edge_modifier() {
        let result = RollPlan::new(
            numeric_pool(),
            vec![RollModifier::Edge {
                count: 1,
                policy: EdgePolicy::RerollLowest,
            }],
            ScoringMode::Sum,
            vec![],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_rejects_non_marvel_plan_with_marvel_fantastic_annotation() {
        let result = RollPlan::new(
            numeric_pool(),
            vec![],
            ScoringMode::Sum,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(matches!(result, Err(Error::InvalidMarvelRoll(_))));
    }

    #[test]
    fn new_accepts_valid_marvel_plan_with_edge_modifier() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![RollModifier::Edge {
                count: 1,
                policy: EdgePolicy::RerollLowest,
            }],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn new_accepts_valid_marvel_plan_with_trouble_and_edge_modifiers() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![
                RollModifier::Edge {
                    count: 1,
                    policy: EdgePolicy::RerollLowest,
                },
                RollModifier::Trouble { count: 1 },
            ],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn new_accepts_valid_marvel_plan_with_no_modifiers() {
        let result = RollPlan::new(
            marvel_pool(),
            vec![],
            ScoringMode::MarvelMultiverse,
            vec![AnnotationRule::MarvelFantastic],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn new_accepts_valid_non_marvel_plan() {
        let pool = DicePool {
            count: 6,
            kind: DieKind::Number(6),
        };
        let result = RollPlan::new(pool, vec![], ScoringMode::Sum, vec![]);
        assert!(result.is_ok());
    }

    #[test]
    fn new_accepts_non_marvel_plan_with_critical_success_annotation() {
        let pool = DicePool {
            count: 1,
            kind: DieKind::Number(20),
        };
        let result = RollPlan::new(
            pool,
            vec![],
            ScoringMode::Sum,
            vec![AnnotationRule::CriticalSuccess(Condition {
                compare: Compare::Equal,
                value: 20,
            })],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn new_unchecked_builds_without_validation() {
        // Deliberately invalid: MarvelD6 kind paired with Sum scoring.
        let plan = RollPlan::new_unchecked(marvel_pool(), vec![], ScoringMode::Sum, vec![]);
        assert_eq!(plan.pools(), &[marvel_pool()]);
        assert_eq!(plan.modifiers(), &[]);
        assert_eq!(plan.scoring(), &ScoringMode::Sum);
        assert_eq!(plan.annotation_rules(), &[]);
    }

    fn marvel_face() -> DieFace {
        DieFace::Symbols(SymbolPool::of(&[Symbol::Marvel]))
    }

    #[test]
    fn as_numeric_is_some_for_numeric_face() {
        assert_eq!(DieFace::Numeric(4).as_numeric(), Some(4));
    }

    #[test]
    fn as_numeric_is_none_for_symbol_face() {
        assert_eq!(marvel_face().as_numeric(), None);
    }

    #[test]
    fn numeric_value_returns_value_for_numeric_face() {
        assert_eq!(DieFace::Numeric(4).numeric_value(), 4);
    }

    #[test]
    #[should_panic(expected = "numeric by construction")]
    fn numeric_value_panics_on_symbol_face() {
        let _ = marvel_face().numeric_value();
    }

    #[test]
    fn marvel_face_displays_as_m() {
        assert_eq!(marvel_face().to_string(), "M");
    }

    #[test]
    fn blank_symbol_face_displays_as_dash() {
        assert_eq!(DieFace::Symbols(SymbolPool::new()).to_string(), "-");
    }

    #[test]
    fn multi_symbol_face_displays_in_declaration_order_with_repeats() {
        let face = DieFace::Symbols(SymbolPool::of(&[
            Symbol::Advantage,
            Symbol::Success,
            Symbol::Advantage,
            Symbol::Triumph,
        ]));
        assert_eq!(face.to_string(), "SAATr");
    }

    #[test]
    fn symbol_all_entries_sit_at_their_own_index() {
        for (i, s) in Symbol::ALL.iter().enumerate() {
            assert_eq!(s.index(), i, "Symbol::ALL is out of sync with index()");
        }
    }

    #[test]
    fn symbol_pool_round_trips_every_symbol() {
        for &s in Symbol::ALL.iter() {
            let pool = SymbolPool::of(&[s]);
            assert_eq!(pool.count(s), 1);
            for &other in Symbol::ALL.iter() {
                if other != s {
                    assert_eq!(pool.count(other), 0);
                }
            }
        }
    }

    #[test]
    fn symbol_pool_of_counts_repeated_symbols() {
        let pool = SymbolPool::of(&[Symbol::Success, Symbol::Success, Symbol::Advantage]);
        assert_eq!(pool.count(Symbol::Success), 2);
        assert_eq!(pool.count(Symbol::Advantage), 1);
        assert_eq!(pool.count(Symbol::Triumph), 0);
    }

    #[test]
    fn symbol_pool_new_is_empty_and_equals_default() {
        let pool = SymbolPool::new();
        assert!(pool.is_empty());
        assert_eq!(pool, SymbolPool::default());
        for &s in Symbol::ALL.iter() {
            assert_eq!(pool.count(s), 0);
            assert!(!pool.contains(s));
        }
    }

    #[test]
    fn symbol_pool_contains_reflects_nonzero_count() {
        let pool = SymbolPool::of(&[Symbol::Threat]);
        assert!(pool.contains(Symbol::Threat));
        assert!(!pool.contains(Symbol::Despair));
        assert!(!pool.is_empty());
    }

    #[test]
    fn symbol_pool_add_saturates_at_u8_max() {
        let mut pool = SymbolPool::new();
        pool.add(Symbol::Light, u8::MAX);
        pool.add(Symbol::Light, 1);
        assert_eq!(pool.count(Symbol::Light), u8::MAX);
    }

    #[test]
    fn symbol_pool_merge_sums_counts() {
        let mut a = SymbolPool::of(&[Symbol::Success, Symbol::Advantage]);
        let b = SymbolPool::of(&[Symbol::Success, Symbol::Failure]);
        a.merge(&b);
        assert_eq!(a.count(Symbol::Success), 2);
        assert_eq!(a.count(Symbol::Advantage), 1);
        assert_eq!(a.count(Symbol::Failure), 1);
        assert_eq!(a.count(Symbol::Threat), 0);
    }

    #[test]
    fn symbol_pool_merge_saturates_at_u8_max() {
        let mut a = SymbolPool::new();
        a.add(Symbol::Dark, u8::MAX);
        let mut b = SymbolPool::new();
        b.add(Symbol::Dark, 5);
        a.merge(&b);
        assert_eq!(a.count(Symbol::Dark), u8::MAX);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn symbol_pool_serializes_to_name_count_map_omitting_zeros() {
        let pool = SymbolPool::of(&[Symbol::Success, Symbol::Success, Symbol::Threat]);
        let json = serde_json::to_value(pool).unwrap();
        assert_eq!(json, serde_json::json!({"Success": 2, "Threat": 1}));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn symbol_pool_empty_serializes_to_empty_map() {
        let json = serde_json::to_value(SymbolPool::new()).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn symbol_pool_deserializes_from_name_count_map() {
        let json = serde_json::json!({"Success": 2, "Threat": 1});
        let pool: SymbolPool = serde_json::from_value(json).unwrap();
        assert_eq!(pool.count(Symbol::Success), 2);
        assert_eq!(pool.count(Symbol::Threat), 1);
        assert_eq!(pool.count(Symbol::Advantage), 0);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn symbols_outcome_serializes_with_kind_tag() {
        let outcome = RollOutcome::Symbols(SymbolsOutcome {
            successes: 2,
            advantages: -1,
            triumphs: 1,
            despairs: 0,
            light: 0,
            dark: 2,
        });
        let json = serde_json::to_value(outcome).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "Symbols": {
                    "successes": 2,
                    "advantages": -1,
                    "triumphs": 1,
                    "despairs": 0,
                    "light": 0,
                    "dark": 2,
                }
            })
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn narrative_annotations_serialize_as_names() {
        let annotations = vec![Annotation::Triumph, Annotation::Despair];
        let json = serde_json::to_value(&annotations).unwrap();
        assert_eq!(json, serde_json::json!(["Triumph", "Despair"]));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn symbol_pool_serde_round_trips() {
        let pool = SymbolPool::of(&[Symbol::Triumph, Symbol::Despair, Symbol::Despair]);
        let json = serde_json::to_string(&pool).unwrap();
        let restored: SymbolPool = serde_json::from_str(&json).unwrap();
        assert_eq!(pool, restored);
    }
}
