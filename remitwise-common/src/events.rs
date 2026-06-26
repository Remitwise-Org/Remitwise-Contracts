#![doc = include_str!("../../docs/EVENTS.md")]

/// Primary contract topic symbols
pub const TOPIC_REMITWISE: &str = "Remitwise";
pub const TOPIC_SAVINGS: &str = "savings";
pub const TOPIC_GOALS: &str = "goals";
pub const TOPIC_INSURE: &str = "insure";
pub const TOPIC_SPLIT: &str = "split";
pub const TOPIC_SCHEDULE: &str = "schedule";
pub const TOPIC_FAMILY: &str = "family";
pub const TOPIC_ORCH: &str = "orch";
pub const TOPIC_REPORTING: &str = "reporting";

/// Action symbols
pub const ACTION_PAID: &str = "paid";
pub const ACTION_CANCELLED: &str = "cancelled";
pub const ACTION_EXT_UPD: &str = "ext_upd";
pub const ACTION_ARCHIVE: &str = "archive";
pub const ACTION_RESTORED: &str = "restored";
pub const ACTION_PAUSED: &str = "paused";
pub const ACTION_UNPAUSED: &str = "unpaused";
pub const ACTION_UPGRADED: &str = "upgraded";
pub const ACTION_CREATED: &str = "created";
pub const ACTION_ADDED: &str = "added";
pub const ACTION_COMPLETED: &str = "completed";
pub const ACTION_WITHDRAWN: &str = "withdrawn";
pub const ACTION_DEACTIVE: &str = "deactive";
pub const ACTION_INIT: &str = "init";
pub const ACTION_CALC: &str = "calc";
pub const ACTION_SCH_EXEC: &str = "sch_exec";
pub const ACTION_SCH_MISS: &str = "sch_miss";
pub const ACTION_MEMBER_ADDED: &str = "member_added";
pub const ACTION_LIMIT_UPDATED: &str = "limit_updated";
pub const ACTION_TX_PROPOSED: &str = "tx_proposed";
pub const ACTION_TX_EXECUTED: &str = "tx_executed";
pub const ACTION_EMERGENCY_ON: &str = "emergency_on";
pub const ACTION_EMERGENCY_OFF: &str = "emergency_off";
pub const ACTION_MS_CONF: &str = "ms_conf";
pub const ACTION_FLOW: &str = "flow";
pub const ACTION_FLOW_OK: &str = "flow_ok";
pub const ACTION_FLOW_FAIL: &str = "flow_fail";
pub const ACTION_INIT_OK: &str = "init_ok";
pub const ACTION_BATCH: &str = "batch";
pub const ACTION_SNAP_EXP: &str = "snap_exp";
