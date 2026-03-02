use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::Context;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::ast::{decode_internal_const, decode_internal_type_alias, BinOp, UnaryOp};
use crate::diagnostics::Diagnostic;
use crate::ir;
use crate::telemetry;

mod runtime;
use self::runtime::runtime_c_source;

mod generator_collections;
mod generator_concurrency;
mod generator_control_flow;
mod generator_core;
mod generator_crypto_web;
mod generator_fs_env;
mod generator_io;
mod generator_json_regex;
mod generator_net_tls_buffer;
mod generator_path_proc;
mod generator_string_math;

#[cfg(test)]
mod tests;

const TUPLE_INTERNAL_NAME: &str = "Tuple";

#[derive(Debug, Clone, Copy)]
pub struct IntrinsicSignatureShape {
    pub params: &'static [&'static str],
    pub ret: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct IntrinsicBindingExpectation {
    pub intrinsic: &'static str,
    pub runtime_symbol: &'static str,
    pub signatures: &'static [IntrinsicSignatureShape],
}

const INTRINSIC_BINDING_EXPECTATIONS: &[IntrinsicBindingExpectation] = &[
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_spawn_intrinsic",
        runtime_symbol: "aic_rt_conc_spawn",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[Task[Int], ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_join_intrinsic",
        runtime_symbol: "aic_rt_conc_join",
        signatures: &[IntrinsicSignatureShape {
            params: &["Task[Int]"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_join_timeout_intrinsic",
        runtime_symbol: "aic_rt_conc_join_timeout",
        signatures: &[IntrinsicSignatureShape {
            params: &["Task[Int]", "Int"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_cancel_intrinsic",
        runtime_symbol: "aic_rt_conc_cancel",
        signatures: &[IntrinsicSignatureShape {
            params: &["Task[Int]"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_spawn_fn_intrinsic",
        runtime_symbol: "aic_rt_conc_spawn_fn",
        signatures: &[IntrinsicSignatureShape {
            params: &["Fn[T]"],
            ret: "Result[Task[T], ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_spawn_fn_named_intrinsic",
        runtime_symbol: "aic_rt_conc_spawn_fn_named",
        signatures: &[IntrinsicSignatureShape {
            params: &["String", "Fn[T]"],
            ret: "Result[Task[T], ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_join_value_intrinsic",
        runtime_symbol: "aic_rt_conc_join_value",
        signatures: &[IntrinsicSignatureShape {
            params: &["Task[T]"],
            ret: "Result[T, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_scope_new_intrinsic",
        runtime_symbol: "aic_rt_conc_scope_new",
        signatures: &[IntrinsicSignatureShape {
            params: &[],
            ret: "Result[Scope, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_scope_spawn_fn_intrinsic",
        runtime_symbol: "aic_rt_conc_scope_spawn_fn",
        signatures: &[IntrinsicSignatureShape {
            params: &["Scope", "Fn[T]"],
            ret: "Result[Task[T], ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_scope_join_all_intrinsic",
        runtime_symbol: "aic_rt_conc_scope_join_all",
        signatures: &[IntrinsicSignatureShape {
            params: &["Scope"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_scope_cancel_intrinsic",
        runtime_symbol: "aic_rt_conc_scope_cancel",
        signatures: &[IntrinsicSignatureShape {
            params: &["Scope"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_scope_close_intrinsic",
        runtime_symbol: "aic_rt_conc_scope_close",
        signatures: &[IntrinsicSignatureShape {
            params: &["Scope"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_spawn_group_intrinsic",
        runtime_symbol: "aic_rt_conc_spawn_group",
        signatures: &[IntrinsicSignatureShape {
            params: &["Vec[Int]", "Int"],
            ret: "Result[Vec[Int], ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_select_first_intrinsic",
        runtime_symbol: "aic_rt_conc_select_first",
        signatures: &[IntrinsicSignatureShape {
            params: &["Vec[Task[Int]]", "Int"],
            ret: "Result[IntTaskSelection, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_channel_int_intrinsic",
        runtime_symbol: "aic_rt_conc_channel_int",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[IntChannel, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_channel_int_buffered_intrinsic",
        runtime_symbol: "aic_rt_conc_channel_int_buffered",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[IntChannel, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_send_int_intrinsic",
        runtime_symbol: "aic_rt_conc_send_int",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntChannel", "Int", "Int"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_try_send_int_intrinsic",
        runtime_symbol: "aic_rt_conc_try_send_int",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntChannel", "Int"],
            ret: "Result[Bool, ChannelError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_recv_int_intrinsic",
        runtime_symbol: "aic_rt_conc_recv_int",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntChannel", "Int"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_try_recv_int_intrinsic",
        runtime_symbol: "aic_rt_conc_try_recv_int",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntChannel"],
            ret: "Result[Int, ChannelError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_select_recv_int_intrinsic",
        runtime_symbol: "aic_rt_conc_select_recv_int",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntChannel", "IntChannel", "Int"],
            ret: "Result[IntChannelSelection, ChannelError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_close_channel_intrinsic",
        runtime_symbol: "aic_rt_conc_close_channel",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntChannel"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_mutex_int_intrinsic",
        runtime_symbol: "aic_rt_conc_mutex_int",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[IntMutex, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_mutex_lock_intrinsic",
        runtime_symbol: "aic_rt_conc_mutex_lock",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntMutex", "Int"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_mutex_unlock_intrinsic",
        runtime_symbol: "aic_rt_conc_mutex_unlock",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntMutex", "Int"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_mutex_close_intrinsic",
        runtime_symbol: "aic_rt_conc_mutex_close",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntMutex"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_rwlock_int_intrinsic",
        runtime_symbol: "aic_rt_conc_rwlock_int",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[IntRwLock, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_rwlock_read_intrinsic",
        runtime_symbol: "aic_rt_conc_rwlock_read",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntRwLock", "Int"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_rwlock_write_lock_intrinsic",
        runtime_symbol: "aic_rt_conc_rwlock_write_lock",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntRwLock", "Int"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_rwlock_write_unlock_intrinsic",
        runtime_symbol: "aic_rt_conc_rwlock_write_unlock",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntRwLock", "Int"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_rwlock_close_intrinsic",
        runtime_symbol: "aic_rt_conc_rwlock_close",
        signatures: &[IntrinsicSignatureShape {
            params: &["IntRwLock"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_payload_store_intrinsic",
        runtime_symbol: "aic_rt_conc_payload_store",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_payload_store_value_intrinsic",
        runtime_symbol: "aic_rt_conc_payload_store",
        signatures: &[IntrinsicSignatureShape {
            params: &["T"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_payload_take_intrinsic",
        runtime_symbol: "aic_rt_conc_payload_take",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[String, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_payload_take_value_intrinsic",
        runtime_symbol: "aic_rt_conc_payload_take",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Option[T]"],
            ret: "Result[T, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_payload_drop_intrinsic",
        runtime_symbol: "aic_rt_conc_payload_drop",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_arc_new_intrinsic",
        runtime_symbol: "aic_rt_conc_arc_new",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_arc_clone_intrinsic",
        runtime_symbol: "aic_rt_conc_arc_clone",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_arc_get_intrinsic",
        runtime_symbol: "aic_rt_conc_arc_get",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[String, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_arc_strong_count_intrinsic",
        runtime_symbol: "aic_rt_conc_arc_strong_count",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_atomic_int_intrinsic",
        runtime_symbol: "aic_rt_conc_atomic_int_new",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_atomic_load_intrinsic",
        runtime_symbol: "aic_rt_conc_atomic_int_load",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_atomic_store_intrinsic",
        runtime_symbol: "aic_rt_conc_atomic_int_store",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_atomic_add_intrinsic",
        runtime_symbol: "aic_rt_conc_atomic_int_add",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_atomic_sub_intrinsic",
        runtime_symbol: "aic_rt_conc_atomic_int_sub",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_atomic_cas_intrinsic",
        runtime_symbol: "aic_rt_conc_atomic_int_cas",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int", "Int"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_atomic_bool_intrinsic",
        runtime_symbol: "aic_rt_conc_atomic_bool_new",
        signatures: &[IntrinsicSignatureShape {
            params: &["Bool"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_atomic_load_bool_intrinsic",
        runtime_symbol: "aic_rt_conc_atomic_bool_load",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_atomic_store_bool_intrinsic",
        runtime_symbol: "aic_rt_conc_atomic_bool_store",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Bool"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_atomic_swap_bool_intrinsic",
        runtime_symbol: "aic_rt_conc_atomic_bool_swap",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Bool"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_tl_new_intrinsic",
        runtime_symbol: "aic_rt_conc_tl_new",
        signatures: &[IntrinsicSignatureShape {
            params: &["Fn[T]"],
            ret: "Result[Int, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_tl_get_intrinsic",
        runtime_symbol: "aic_rt_conc_tl_get",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[T, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_conc_tl_set_intrinsic",
        runtime_symbol: "aic_rt_conc_tl_set",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "T"],
            ret: "Result[Bool, ConcurrencyError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_proc_spawn_intrinsic",
        runtime_symbol: "aic_rt_proc_spawn",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "Result[Int, ProcError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_proc_wait_intrinsic",
        runtime_symbol: "aic_rt_proc_wait",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Int, ProcError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_proc_kill_intrinsic",
        runtime_symbol: "aic_rt_proc_kill",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, ProcError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_proc_run_intrinsic",
        runtime_symbol: "aic_rt_proc_run",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "Result[ProcOutput, ProcError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_proc_pipe_intrinsic",
        runtime_symbol: "aic_rt_proc_pipe",
        signatures: &[IntrinsicSignatureShape {
            params: &["String", "String"],
            ret: "Result[ProcOutput, ProcError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_proc_run_with_intrinsic",
        runtime_symbol: "aic_rt_proc_run_with",
        signatures: &[IntrinsicSignatureShape {
            params: &["String", "RunOptions"],
            ret: "Result[ProcOutput, ProcError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_proc_is_running_intrinsic",
        runtime_symbol: "aic_rt_proc_is_running",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, ProcError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_proc_current_pid_intrinsic",
        runtime_symbol: "aic_rt_proc_current_pid",
        signatures: &[IntrinsicSignatureShape {
            params: &[],
            ret: "Result[Int, ProcError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_proc_run_timeout_intrinsic",
        runtime_symbol: "aic_rt_proc_run_timeout",
        signatures: &[IntrinsicSignatureShape {
            params: &["String", "Int"],
            ret: "Result[ProcOutput, ProcError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_proc_pipe_chain_intrinsic",
        runtime_symbol: "aic_rt_proc_pipe_chain",
        signatures: &[IntrinsicSignatureShape {
            params: &["Vec[String]"],
            ret: "Result[ProcOutput, ProcError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_listen_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_listen",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "Result[Int, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_local_addr_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_local_addr",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[String, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_accept_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_accept",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[Int, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_connect_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_connect",
        signatures: &[IntrinsicSignatureShape {
            params: &["String", "Int"],
            ret: "Result[Int, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_send_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_send",
        signatures: &[
            IntrinsicSignatureShape {
                params: &["Int", "String"],
                ret: "Result[Int, NetError]",
            },
            IntrinsicSignatureShape {
                params: &["Int", "Bytes"],
                ret: "Result[Int, NetError]",
            },
        ],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_send_timeout_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_send_timeout",
        signatures: &[
            IntrinsicSignatureShape {
                params: &["Int", "String", "Int"],
                ret: "Result[Int, NetError]",
            },
            IntrinsicSignatureShape {
                params: &["Int", "Bytes", "Int"],
                ret: "Result[Int, NetError]",
            },
        ],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_recv_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_recv",
        signatures: &[
            IntrinsicSignatureShape {
                params: &["Int", "Int", "Int"],
                ret: "Result[String, NetError]",
            },
            IntrinsicSignatureShape {
                params: &["Int", "Int", "Int"],
                ret: "Result[Bytes, NetError]",
            },
        ],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_close_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_close",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_set_nodelay_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_set_nodelay",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Bool"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_get_nodelay_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_get_nodelay",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_set_keepalive_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_set_keepalive",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Bool"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_get_keepalive_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_get_keepalive",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_set_keepalive_idle_secs_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_set_keepalive_idle_secs",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_get_keepalive_idle_secs_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_get_keepalive_idle_secs",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Int, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_set_keepalive_interval_secs_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_set_keepalive_interval_secs",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_get_keepalive_interval_secs_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_get_keepalive_interval_secs",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Int, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_set_keepalive_count_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_set_keepalive_count",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_get_keepalive_count_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_get_keepalive_count",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Int, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_peer_addr_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_peer_addr",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[String, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_shutdown_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_shutdown",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_shutdown_read_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_shutdown_read",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_shutdown_write_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_shutdown_write",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_set_send_buffer_size_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_set_send_buffer_size",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_get_send_buffer_size_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_get_send_buffer_size",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Int, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_set_recv_buffer_size_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_set_recv_buffer_size",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_tcp_get_recv_buffer_size_intrinsic",
        runtime_symbol: "aic_rt_net_tcp_get_recv_buffer_size",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Int, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_udp_bind_intrinsic",
        runtime_symbol: "aic_rt_net_udp_bind",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "Result[Int, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_udp_local_addr_intrinsic",
        runtime_symbol: "aic_rt_net_udp_local_addr",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[String, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_udp_send_to_intrinsic",
        runtime_symbol: "aic_rt_net_udp_send_to",
        signatures: &[
            IntrinsicSignatureShape {
                params: &["Int", "String", "String"],
                ret: "Result[Int, NetError]",
            },
            IntrinsicSignatureShape {
                params: &["Int", "String", "Bytes"],
                ret: "Result[Int, NetError]",
            },
        ],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_udp_recv_from_intrinsic",
        runtime_symbol: "aic_rt_net_udp_recv_from",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int", "Int"],
            ret: "Result[UdpPacket, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_udp_close_intrinsic",
        runtime_symbol: "aic_rt_net_udp_close",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_dns_lookup_intrinsic",
        runtime_symbol: "aic_rt_net_dns_lookup",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "Result[String, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_dns_lookup_all_intrinsic",
        runtime_symbol: "aic_rt_net_dns_lookup_all",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "Result[Vec[String], NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_dns_reverse_intrinsic",
        runtime_symbol: "aic_rt_net_dns_reverse",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "Result[String, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_async_accept_submit_intrinsic",
        runtime_symbol: "aic_rt_net_async_accept_submit",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[AsyncIntOp, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_async_send_submit_intrinsic",
        runtime_symbol: "aic_rt_net_async_send_submit",
        signatures: &[
            IntrinsicSignatureShape {
                params: &["Int", "String"],
                ret: "Result[AsyncIntOp, NetError]",
            },
            IntrinsicSignatureShape {
                params: &["Int", "Bytes"],
                ret: "Result[AsyncIntOp, NetError]",
            },
        ],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_async_recv_submit_intrinsic",
        runtime_symbol: "aic_rt_net_async_recv_submit",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int", "Int"],
            ret: "Result[AsyncStringOp, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_async_wait_int_intrinsic",
        runtime_symbol: "aic_rt_net_async_wait_int",
        signatures: &[IntrinsicSignatureShape {
            params: &["AsyncIntOp", "Int"],
            ret: "Result[Int, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_async_wait_string_intrinsic",
        runtime_symbol: "aic_rt_net_async_wait_string",
        signatures: &[
            IntrinsicSignatureShape {
                params: &["AsyncStringOp", "Int"],
                ret: "Result[String, NetError]",
            },
            IntrinsicSignatureShape {
                params: &["AsyncStringOp", "Int"],
                ret: "Result[Bytes, NetError]",
            },
        ],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_async_cancel_int_intrinsic",
        runtime_symbol: "aic_rt_net_async_cancel",
        signatures: &[IntrinsicSignatureShape {
            params: &["AsyncIntOp"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_async_cancel_string_intrinsic",
        runtime_symbol: "aic_rt_net_async_cancel",
        signatures: &[IntrinsicSignatureShape {
            params: &["AsyncStringOp"],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_net_async_shutdown_intrinsic",
        runtime_symbol: "aic_rt_net_async_shutdown",
        signatures: &[IntrinsicSignatureShape {
            params: &[],
            ret: "Result[Bool, NetError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_connect_intrinsic",
        runtime_symbol: "aic_rt_tls_connect",
        signatures: &[IntrinsicSignatureShape {
            params: &[
                "Int", "Bool", "String", "Bool", "String", "Bool", "String", "Bool", "String",
                "Bool",
            ],
            ret: "Result[Int, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_connect_addr_intrinsic",
        runtime_symbol: "aic_rt_tls_connect_addr",
        signatures: &[IntrinsicSignatureShape {
            params: &[
                "String", "Bool", "String", "Bool", "String", "Bool", "String", "Bool", "String",
                "Bool", "Int",
            ],
            ret: "Result[Int, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_accept_intrinsic",
        runtime_symbol: "aic_rt_tls_accept",
        signatures: &[IntrinsicSignatureShape {
            params: &[
                "Int", "Bool", "String", "Bool", "String", "Bool", "String", "Bool", "Int",
            ],
            ret: "Result[Int, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_send_intrinsic",
        runtime_symbol: "aic_rt_tls_send",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "String"],
            ret: "Result[Int, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_send_timeout_intrinsic",
        runtime_symbol: "aic_rt_tls_send_timeout",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "String", "Int"],
            ret: "Result[Int, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_recv_intrinsic",
        runtime_symbol: "aic_rt_tls_recv",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int", "Int"],
            ret: "Result[String, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_async_send_submit_intrinsic",
        runtime_symbol: "aic_rt_tls_async_send_submit",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "String", "Int"],
            ret: "Result[AsyncIntOp, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_async_recv_submit_intrinsic",
        runtime_symbol: "aic_rt_tls_async_recv_submit",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int", "Int"],
            ret: "Result[AsyncStringOp, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_async_wait_int_intrinsic",
        runtime_symbol: "aic_rt_tls_async_wait_int",
        signatures: &[IntrinsicSignatureShape {
            params: &["AsyncIntOp", "Int"],
            ret: "Result[Int, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_async_wait_string_intrinsic",
        runtime_symbol: "aic_rt_tls_async_wait_string",
        signatures: &[
            IntrinsicSignatureShape {
                params: &["AsyncStringOp", "Int"],
                ret: "Result[String, TlsError]",
            },
            IntrinsicSignatureShape {
                params: &["AsyncStringOp", "Int"],
                ret: "Result[Bytes, TlsError]",
            },
        ],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_async_cancel_int_intrinsic",
        runtime_symbol: "aic_rt_tls_async_cancel",
        signatures: &[IntrinsicSignatureShape {
            params: &["AsyncIntOp"],
            ret: "Result[Bool, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_async_cancel_string_intrinsic",
        runtime_symbol: "aic_rt_tls_async_cancel",
        signatures: &[IntrinsicSignatureShape {
            params: &["AsyncStringOp"],
            ret: "Result[Bool, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_async_shutdown_intrinsic",
        runtime_symbol: "aic_rt_tls_async_shutdown",
        signatures: &[IntrinsicSignatureShape {
            params: &[],
            ret: "Result[Bool, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_close_intrinsic",
        runtime_symbol: "aic_rt_tls_close",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Bool, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_peer_subject_intrinsic",
        runtime_symbol: "aic_rt_tls_peer_subject",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[String, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_peer_issuer_intrinsic",
        runtime_symbol: "aic_rt_tls_peer_issuer",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[String, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_peer_fingerprint_sha256_intrinsic",
        runtime_symbol: "aic_rt_tls_peer_fingerprint_sha256",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[String, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_peer_san_entries_intrinsic",
        runtime_symbol: "aic_rt_tls_peer_san_entries",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Vec[String], TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_tls_version_intrinsic",
        runtime_symbol: "aic_rt_tls_version",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "Result[Int, TlsError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_bytes_byte_at_intrinsic",
        runtime_symbol: "aic_rt_bytes_byte_at",
        signatures: &[IntrinsicSignatureShape {
            params: &["String", "Int"],
            ret: "Int",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_bytes_from_byte_values_intrinsic",
        runtime_symbol: "aic_rt_bytes_from_byte_values",
        signatures: &[IntrinsicSignatureShape {
            params: &["Vec[Int]"],
            ret: "String",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_new_intrinsic",
        runtime_symbol: "aic_rt_buffer_new",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "ByteBuffer",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_new_growable_intrinsic",
        runtime_symbol: "aic_rt_buffer_new_growable",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int", "Int"],
            ret: "Result[ByteBuffer, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_from_bytes_intrinsic",
        runtime_symbol: "aic_rt_buffer_from_bytes",
        signatures: &[IntrinsicSignatureShape {
            params: &["Bytes"],
            ret: "ByteBuffer",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_to_bytes_intrinsic",
        runtime_symbol: "aic_rt_buffer_to_bytes",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Bytes",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_position_intrinsic",
        runtime_symbol: "aic_rt_buffer_position",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Int",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_remaining_intrinsic",
        runtime_symbol: "aic_rt_buffer_remaining",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Int",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_seek_intrinsic",
        runtime_symbol: "aic_rt_buffer_seek",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_reset_intrinsic",
        runtime_symbol: "aic_rt_buffer_reset",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "()",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_close_intrinsic",
        runtime_symbol: "aic_rt_buffer_close",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Bool, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_u8_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_u8",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_i16_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_i16_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_u16_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_u16_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_i32_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_i32_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_u32_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_u32_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_i64_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_i64_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_u64_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_u64_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_i16_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_i16_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_u16_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_u16_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_i32_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_i32_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_u32_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_u32_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_i64_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_i64_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_u64_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_u64_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Int, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_bytes_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_bytes",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[Bytes, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_cstring_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_cstring",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[String, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_read_length_prefixed_intrinsic",
        runtime_symbol: "aic_rt_buffer_read_length_prefixed",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer"],
            ret: "Result[Bytes, BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_u8_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_u8",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_i16_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_i16_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_u16_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_u16_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_i32_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_i32_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_u32_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_u32_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_i64_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_i64_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_u64_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_u64_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_i16_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_i16_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_u16_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_u16_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_i32_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_i32_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_u32_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_u32_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_i64_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_i64_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_u64_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_u64_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_bytes_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_bytes",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Bytes"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_cstring_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_cstring",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "String"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_write_string_prefixed_intrinsic",
        runtime_symbol: "aic_rt_buffer_write_string_prefixed",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "String"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_patch_u16_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_patch_u16_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_patch_u32_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_patch_u32_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_patch_u64_be_intrinsic",
        runtime_symbol: "aic_rt_buffer_patch_u64_be",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_patch_u16_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_patch_u16_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_patch_u32_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_patch_u32_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_buffer_patch_u64_le_intrinsic",
        runtime_symbol: "aic_rt_buffer_patch_u64_le",
        signatures: &[IntrinsicSignatureShape {
            params: &["ByteBuffer", "Int", "Int"],
            ret: "Result[(), BufferError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_md5_intrinsic",
        runtime_symbol: "aic_rt_crypto_md5",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "String",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_sha256_intrinsic",
        runtime_symbol: "aic_rt_crypto_sha256",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "String",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_sha256_raw_intrinsic",
        runtime_symbol: "aic_rt_crypto_sha256_raw",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "String",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_hmac_sha256_intrinsic",
        runtime_symbol: "aic_rt_crypto_hmac_sha256",
        signatures: &[IntrinsicSignatureShape {
            params: &["String", "String"],
            ret: "String",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_hmac_sha256_raw_intrinsic",
        runtime_symbol: "aic_rt_crypto_hmac_sha256_raw",
        signatures: &[IntrinsicSignatureShape {
            params: &["String", "String"],
            ret: "String",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_pbkdf2_sha256_intrinsic",
        runtime_symbol: "aic_rt_crypto_pbkdf2_sha256",
        signatures: &[IntrinsicSignatureShape {
            params: &["String", "String", "Int", "Int"],
            ret: "Result[String, CryptoError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_hex_encode_intrinsic",
        runtime_symbol: "aic_rt_crypto_hex_encode",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "String",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_hex_decode_intrinsic",
        runtime_symbol: "aic_rt_crypto_hex_decode",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "Result[String, CryptoError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_base64_encode_intrinsic",
        runtime_symbol: "aic_rt_crypto_base64_encode",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "String",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_base64_decode_intrinsic",
        runtime_symbol: "aic_rt_crypto_base64_decode",
        signatures: &[IntrinsicSignatureShape {
            params: &["String"],
            ret: "Result[String, CryptoError]",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_random_bytes_intrinsic",
        runtime_symbol: "aic_rt_crypto_random_bytes",
        signatures: &[IntrinsicSignatureShape {
            params: &["Int"],
            ret: "String",
        }],
    },
    IntrinsicBindingExpectation {
        intrinsic: "aic_crypto_secure_eq_intrinsic",
        runtime_symbol: "aic_rt_crypto_secure_eq",
        signatures: &[IntrinsicSignatureShape {
            params: &["String", "String"],
            ret: "Bool",
        }],
    },
];

pub fn intrinsic_binding_expectations() -> &'static [IntrinsicBindingExpectation] {
    INTRINSIC_BINDING_EXPECTATIONS
}

pub fn intrinsic_binding_expectation(name: &str) -> Option<&'static IntrinsicBindingExpectation> {
    INTRINSIC_BINDING_EXPECTATIONS
        .iter()
        .find(|binding| binding.intrinsic == name)
}

#[derive(Debug, Clone)]
struct FnSig {
    is_extern: bool,
    extern_symbol: Option<String>,
    extern_abi: Option<String>,
    is_intrinsic: bool,
    intrinsic_abi: Option<String>,
    params: Vec<LType>,
    ret: LType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LType {
    Int,
    Float,
    Bool,
    Char,
    Unit,
    String,
    Fn(FnLayoutType),
    DynTrait(String),
    Struct(StructLayoutType),
    Enum(EnumLayoutType),
    Async(Box<LType>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FnLayoutType {
    repr: String,
    params: Vec<LType>,
    ret: Box<LType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StructLayoutType {
    repr: String,
    fields: Vec<StructFieldType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StructFieldType {
    name: String,
    ty: LType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumLayoutType {
    repr: String,
    variants: Vec<EnumVariantType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumVariantType {
    name: String,
    payload: Option<LType>,
}

#[derive(Debug, Clone)]
struct DynTraitMethodInfo {
    name: String,
    params: Vec<LType>,
    ret: LType,
}

#[derive(Debug, Clone)]
struct DynTraitInfo {
    methods: Vec<DynTraitMethodInfo>,
    method_index: BTreeMap<String, usize>,
    impl_methods: BTreeMap<String, BTreeMap<String, ir::SymbolId>>,
}

#[derive(Debug, Clone)]
struct StructTemplate {
    generics: Vec<String>,
    fields: Vec<(String, String)>,
    field_defaults: BTreeMap<String, ir::Expr>,
}

#[derive(Debug, Clone)]
struct EnumTemplate {
    generics: Vec<String>,
    variants: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone)]
struct VariantCtor {
    enum_name: String,
    variant_index: usize,
}

#[derive(Debug, Clone)]
struct GenericFnInstance {
    mangled: String,
    params: Vec<LType>,
    ret: LType,
    bindings: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct AliasDef {
    generics: Vec<String>,
    target: String,
    span: crate::span::Span,
}

#[derive(Debug, Clone)]
struct ConstDef {
    declared_ty: String,
    init: Option<ir::Expr>,
    span: crate::span::Span,
}

#[derive(Debug, Clone)]
enum ConstValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    Unit,
    String(String),
}

#[derive(Debug, Clone)]
struct Value {
    ty: LType,
    repr: Option<String>,
}

#[derive(Debug, Clone)]
struct ValueWithErr {
    value: Value,
    err_code: String,
}

#[derive(Debug, Clone)]
struct JsonObjectGetValue {
    value: Value,
    found: String,
    err_code: String,
}

#[derive(Debug, Clone)]
struct Local {
    symbol: Option<ir::SymbolId>,
    ty: LType,
    ptr: String,
}

#[derive(Debug, Clone)]
struct SourceMap {
    line_starts: Vec<usize>,
    source_len: usize,
}

impl SourceMap {
    fn from_source(source: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (idx, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }
        Self {
            line_starts,
            source_len: source.len(),
        }
    }

    fn line_col(&self, offset: usize) -> (u64, u64) {
        if self.line_starts.is_empty() {
            return (0, 0);
        }
        let max_offset = offset.min(self.source_len);
        let idx = self
            .line_starts
            .partition_point(|start| *start <= max_offset);
        let line_index = idx.saturating_sub(1);
        let line_start = self.line_starts[line_index];
        let line = (line_index + 1) as u64;
        let column = (max_offset.saturating_sub(line_start) + 1) as u64;
        (line, column)
    }
}

#[derive(Debug, Clone)]
struct DebugState {
    metadata: Vec<String>,
    file_id: usize,
    compile_unit_id: usize,
    subroutine_type_id: usize,
    next_id: usize,
}

impl DebugState {
    fn new(file: &str) -> Self {
        let path = Path::new(file);
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(file);
        let directory = path
            .parent()
            .and_then(|dir| dir.to_str())
            .filter(|dir| !dir.is_empty())
            .unwrap_or(".");

        let file_name = escape_llvm_string(file_name);
        let directory = escape_llvm_string(directory);

        let compile_unit_id = 0usize;
        let file_id = 1usize;
        let empty_type_list_id = 2usize;
        let dwarf_flag_id = 3usize;
        let debug_version_flag_id = 4usize;
        let ident_id = 5usize;
        let subroutine_type_id = 6usize;

        let metadata = vec![
            format!("!llvm.dbg.cu = !{{!{compile_unit_id}}}"),
            format!("!llvm.module.flags = !{{!{dwarf_flag_id}, !{debug_version_flag_id}}}"),
            format!("!llvm.ident = !{{!{ident_id}}}"),
            format!(
                "!{compile_unit_id} = distinct !DICompileUnit(language: DW_LANG_C, file: !{file_id}, producer: \"aicore\", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug)"
            ),
            format!("!{file_id} = !DIFile(filename: \"{file_name}\", directory: \"{directory}\")"),
            format!("!{empty_type_list_id} = !{{}}"),
            format!("!{dwarf_flag_id} = !{{i32 2, !\"Dwarf Version\", i32 5}}"),
            format!("!{debug_version_flag_id} = !{{i32 2, !\"Debug Info Version\", i32 3}}"),
            format!("!{ident_id} = !{{!\"aicore\"}}"),
            format!("!{subroutine_type_id} = !DISubroutineType(types: !{empty_type_list_id})"),
        ];

        Self {
            metadata,
            file_id,
            compile_unit_id,
            subroutine_type_id,
            next_id: 7,
        }
    }

    fn next_metadata_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn push_node(&mut self, node_text: String) -> usize {
        let id = self.next_metadata_id();
        self.metadata.push(format!("!{id} = {node_text}"));
        id
    }

    fn new_subprogram(&mut self, source_name: &str, linkage_name: &str, line: u64) -> usize {
        let line = line.max(1);
        let source_name = escape_llvm_string(source_name);
        let linkage_name = escape_llvm_string(linkage_name);
        self.push_node(format!(
            "distinct !DISubprogram(name: \"{source_name}\", linkageName: \"{linkage_name}\", scope: !{}, file: !{}, line: {}, type: !{}, scopeLine: {}, spFlags: DISPFlagDefinition, unit: !{})",
            self.file_id,
            self.file_id,
            line,
            self.subroutine_type_id,
            line,
            self.compile_unit_id
        ))
    }

    fn new_location(&mut self, line: u64, column: u64, scope: usize) -> usize {
        let line = line.max(1);
        let column = column.max(1);
        self.push_node(format!(
            "!DILocation(line: {line}, column: {column}, scope: !{scope})"
        ))
    }
}

pub struct CodegenOutput {
    pub llvm_ir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodegenOptions {
    pub debug_info: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptimizationLevel {
    #[default]
    O0,
    O1,
    O2,
    O3,
}

impl OptimizationLevel {
    pub fn clang_flag(self) -> &'static str {
        match self {
            OptimizationLevel::O0 => "-O0",
            OptimizationLevel::O1 => "-O1",
            OptimizationLevel::O2 => "-O2",
            OptimizationLevel::O3 => "-O3",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            OptimizationLevel::O0 => "O0",
            OptimizationLevel::O1 => "O1",
            OptimizationLevel::O2 => "O2",
            OptimizationLevel::O3 => "O3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    Exe,
    Obj,
    Lib,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LinkOptions {
    pub search_paths: Vec<PathBuf>,
    pub libs: Vec<String>,
    pub objects: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompileOptions {
    pub debug_info: bool,
    pub opt_level: OptimizationLevel,
    pub target_triple: Option<String>,
    pub static_link: bool,
    pub link: LinkOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RuntimeInstrumentationOptions {
    pub check_leaks: bool,
    pub asan: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainInfo {
    pub clang_version: String,
    pub llvm_major: u32,
}

pub const MIN_SUPPORTED_LLVM_MAJOR: u32 = 14;

pub fn emit_llvm(program: &ir::Program, file: &str) -> Result<CodegenOutput, Vec<Diagnostic>> {
    emit_llvm_with_options(program, file, CodegenOptions::default())
}

pub fn emit_llvm_with_options(
    program: &ir::Program,
    file: &str,
    options: CodegenOptions,
) -> Result<CodegenOutput, Vec<Diagnostic>> {
    let started = Instant::now();
    let mut gen = Generator::new(program, file, options);
    gen.generate();
    let mut attrs = BTreeMap::from([
        ("file".to_string(), json!(file)),
        ("debug_info".to_string(), json!(options.debug_info)),
    ]);
    if !gen.diagnostics.is_empty() {
        telemetry::emit_phase(
            "codegen",
            "llvm_emit",
            "error",
            started.elapsed(),
            attrs.clone(),
        );
        attrs.insert(
            "diagnostic_count".to_string(),
            json!(gen.diagnostics.len() as u64),
        );
        telemetry::emit_metric(
            "codegen",
            "llvm_emit_diagnostic_count",
            gen.diagnostics.len() as f64,
            attrs,
        );
        return Err(gen.diagnostics);
    }
    telemetry::emit_phase("codegen", "llvm_emit", "ok", started.elapsed(), attrs);
    Ok(CodegenOutput {
        llvm_ir: gen.finish(),
    })
}

pub fn compile_with_clang(
    llvm_ir: &str,
    output_path: &Path,
    work_dir: &Path,
) -> anyhow::Result<PathBuf> {
    compile_with_clang_artifact_with_options(
        llvm_ir,
        output_path,
        work_dir,
        ArtifactKind::Exe,
        CompileOptions::default(),
    )
}

pub fn compile_with_clang_artifact(
    llvm_ir: &str,
    output_path: &Path,
    work_dir: &Path,
    artifact: ArtifactKind,
) -> anyhow::Result<PathBuf> {
    compile_with_clang_artifact_with_options(
        llvm_ir,
        output_path,
        work_dir,
        artifact,
        CompileOptions::default(),
    )
}

pub fn compile_with_clang_artifact_with_options(
    llvm_ir: &str,
    output_path: &Path,
    work_dir: &Path,
    artifact: ArtifactKind,
    options: CompileOptions,
) -> anyhow::Result<PathBuf> {
    compile_with_clang_artifact_with_options_and_runtime(
        llvm_ir,
        output_path,
        work_dir,
        artifact,
        options,
        runtime_instrumentation_from_env(),
    )
}

pub fn compile_with_clang_artifact_with_options_and_runtime(
    llvm_ir: &str,
    output_path: &Path,
    work_dir: &Path,
    artifact: ArtifactKind,
    options: CompileOptions,
    runtime: RuntimeInstrumentationOptions,
) -> anyhow::Result<PathBuf> {
    if options.static_link && artifact != ArtifactKind::Exe {
        anyhow::bail!("--static-link is only supported for executable artifacts");
    }
    let wasm_target = target_is_wasm(options.target_triple.as_deref());
    if wasm_target && artifact != ArtifactKind::Exe {
        anyhow::bail!("wasm32 target currently supports executable artifacts only");
    }
    if wasm_target && options.static_link {
        anyhow::bail!("--static-link is not supported for wasm32 target");
    }
    if wasm_target && runtime.asan {
        anyhow::bail!("AddressSanitizer is not supported for wasm32 target");
    }
    let started = Instant::now();
    let clang_bin = resolve_clang_binary()?;
    let toolchain = probe_toolchain(&clang_bin)?;
    ensure_supported_toolchain(&toolchain)?;

    fs::create_dir_all(work_dir)?;
    ensure_parent_dir(output_path)?;

    let ll_path = work_dir.join("main.ll");
    let runtime_path = work_dir.join("runtime.c");
    let module_obj_path = work_dir.join("module.o");
    let runtime_obj_path = work_dir.join("runtime.o");
    let runtime_flags = runtime_compile_flags(runtime);
    let tls_flags = runtime_tls_compile_flags();
    let deterministic_path_flags = deterministic_source_path_flags(work_dir);
    let mut llvm_to_compile = if runtime.check_leaks {
        instrument_llvm_for_leak_tracking(llvm_ir)
    } else {
        llvm_ir.to_string()
    };
    if wasm_target {
        llvm_to_compile = rewrite_wasm_entry_wrapper(&llvm_to_compile);
    }

    fs::write(&ll_path, llvm_to_compile)?;
    if !wasm_target {
        fs::write(&runtime_path, runtime_c_source())?;
    }

    match artifact {
        ArtifactKind::Exe => {
            let mut command = Command::new(&clang_bin);
            append_target_triple(&mut command, options.target_triple.as_deref());
            if options.debug_info {
                command.arg("-g");
            }
            for flag in &deterministic_path_flags {
                command.arg(flag);
            }
            if wasm_target {
                command
                    .arg(options.opt_level.clang_flag())
                    .arg("-nostdlib")
                    .arg(&ll_path)
                    .arg("-Wl,--no-entry")
                    .arg("-Wl,--allow-undefined")
                    .arg("-Wl,--export=main")
                    .arg("-Wl,--export=aic_main");
                append_link_options(&mut command, &options.link);
            } else {
                for flag in &runtime_flags {
                    command.arg(flag);
                }
                for flag in &tls_flags.cflags {
                    command.arg(flag);
                }
                command.arg(&tls_flags.define_flag);
                command
                    .arg(options.opt_level.clang_flag())
                    .arg(&ll_path)
                    .arg(&runtime_path);
                if options.static_link {
                    if !target_supports_static_link(options.target_triple.as_deref()) {
                        anyhow::bail!(
                            "--static-link currently supports linux targets only (requested: {})",
                            options.target_triple.as_deref().unwrap_or("host target")
                        );
                    }
                    command.arg("-static");
                }
                append_link_options(&mut command, &options.link);
                for flag in &tls_flags.link_flags {
                    command.arg(flag);
                }
                if cfg!(not(target_os = "windows")) {
                    command.arg("-pthread").arg("-lm");
                }
            }
            command.arg("-o").arg(output_path);
            run_checked_command(command, &clang_bin, "building executable artifact")?;
            if !wasm_target && target_is_macos(options.target_triple.as_deref()) {
                normalize_macos_uuid_and_codesign(output_path)?;
            }
        }
        ArtifactKind::Obj => {
            let mut command = Command::new(&clang_bin);
            append_target_triple(&mut command, options.target_triple.as_deref());
            if options.debug_info {
                command.arg("-g");
            }
            for flag in &deterministic_path_flags {
                command.arg(flag);
            }
            for flag in &runtime_flags {
                command.arg(flag);
            }
            command
                .arg(options.opt_level.clang_flag())
                .arg("-c")
                .arg(&ll_path)
                .arg("-o")
                .arg(output_path);
            run_checked_command(command, &clang_bin, "building object artifact")?;
        }
        ArtifactKind::Lib => {
            let mut clang_module = Command::new(&clang_bin);
            append_target_triple(&mut clang_module, options.target_triple.as_deref());
            if options.debug_info {
                clang_module.arg("-g");
            }
            for flag in &deterministic_path_flags {
                clang_module.arg(flag);
            }
            for flag in &runtime_flags {
                clang_module.arg(flag);
            }
            clang_module
                .arg(options.opt_level.clang_flag())
                .arg("-c")
                .arg(&ll_path)
                .arg("-o")
                .arg(&module_obj_path);
            run_checked_command(
                clang_module,
                &clang_bin,
                "building module object for static library",
            )?;

            let mut clang_runtime = Command::new(&clang_bin);
            append_target_triple(&mut clang_runtime, options.target_triple.as_deref());
            if options.debug_info {
                clang_runtime.arg("-g");
            }
            for flag in &deterministic_path_flags {
                clang_runtime.arg(flag);
            }
            for flag in &runtime_flags {
                clang_runtime.arg(flag);
            }
            for flag in &tls_flags.cflags {
                clang_runtime.arg(flag);
            }
            clang_runtime.arg(&tls_flags.define_flag);
            clang_runtime
                .arg(options.opt_level.clang_flag())
                .arg("-c")
                .arg(&runtime_path)
                .arg("-o")
                .arg(&runtime_obj_path);
            run_checked_command(
                clang_runtime,
                &clang_bin,
                "building runtime object for static library",
            )?;

            let ar_bin = std::env::var("AR").unwrap_or_else(|_| "ar".to_string());
            let mut ar = Command::new(&ar_bin);
            ar.arg("rcs")
                .arg(output_path)
                .arg(&module_obj_path)
                .arg(&runtime_obj_path);
            run_checked_command(ar, &ar_bin, "archiving static library artifact")?;
        }
    }

    telemetry::emit_phase("codegen", "clang_compile", "ok", started.elapsed(), {
        let mut attrs = BTreeMap::from([
            (
                "artifact".to_string(),
                json!(match artifact {
                    ArtifactKind::Exe => "exe",
                    ArtifactKind::Obj => "obj",
                    ArtifactKind::Lib => "lib",
                }),
            ),
            (
                "output".to_string(),
                json!(output_path.to_string_lossy().to_string()),
            ),
            ("static_link".to_string(), json!(options.static_link)),
            ("opt_level".to_string(), json!(options.opt_level.as_str())),
            ("check_leaks".to_string(), json!(runtime.check_leaks)),
            ("asan".to_string(), json!(runtime.asan)),
        ]);
        if let Some(triple) = &options.target_triple {
            attrs.insert("target_triple".to_string(), json!(triple));
        }
        attrs
    });

    Ok(output_path.to_path_buf())
}

fn runtime_instrumentation_from_env() -> RuntimeInstrumentationOptions {
    RuntimeInstrumentationOptions {
        check_leaks: env_flag_enabled("AIC_CHECK_LEAKS"),
        asan: env_flag_enabled("AIC_ASAN"),
    }
}

fn env_flag_enabled(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    !matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "no"
    )
}

fn runtime_compile_flags(runtime: RuntimeInstrumentationOptions) -> Vec<&'static str> {
    let mut flags = Vec::new();
    if runtime.check_leaks {
        flags.push("-DAIC_RT_CHECK_LEAKS=1");
    }
    if runtime.asan {
        flags.push("-fsanitize=address");
        flags.push("-fno-omit-frame-pointer");
    }
    flags
}

fn deterministic_source_path_flags(work_dir: &Path) -> Vec<String> {
    let prefix = work_dir.to_string_lossy();
    vec![
        format!("-ffile-prefix-map={prefix}=aic-build"),
        format!("-fmacro-prefix-map={prefix}=aic-build"),
        format!("-fdebug-prefix-map={prefix}=aic-build"),
    ]
}

#[derive(Debug, Clone)]
struct RuntimeTlsCompileFlags {
    define_flag: String,
    cflags: Vec<String>,
    link_flags: Vec<String>,
}

fn runtime_tls_compile_flags() -> RuntimeTlsCompileFlags {
    match probe_pkg_config_flags("openssl") {
        Some((cflags, link_flags)) => RuntimeTlsCompileFlags {
            define_flag: "-DAIC_RT_TLS_OPENSSL=1".to_string(),
            cflags,
            link_flags,
        },
        None => RuntimeTlsCompileFlags {
            define_flag: "-DAIC_RT_TLS_OPENSSL=0".to_string(),
            cflags: Vec::new(),
            link_flags: Vec::new(),
        },
    }
}

fn probe_pkg_config_flags(package: &str) -> Option<(Vec<String>, Vec<String>)> {
    let cflags_output = Command::new("pkg-config")
        .arg("--cflags")
        .arg(package)
        .output()
        .ok()?;
    if !cflags_output.status.success() {
        return None;
    }
    let link_output = Command::new("pkg-config")
        .arg("--libs")
        .arg(package)
        .output()
        .ok()?;
    if !link_output.status.success() {
        return None;
    }
    let cflags = String::from_utf8_lossy(&cflags_output.stdout)
        .split_whitespace()
        .map(|part| part.to_string())
        .collect::<Vec<_>>();
    let link_flags = String::from_utf8_lossy(&link_output.stdout)
        .split_whitespace()
        .map(|part| part.to_string())
        .collect::<Vec<_>>();
    Some((cflags, link_flags))
}

fn instrument_llvm_for_leak_tracking(llvm_ir: &str) -> String {
    llvm_ir.replace("@malloc(", "@aic_rt_heap_alloc(")
}

fn rewrite_wasm_entry_wrapper(llvm_ir: &str) -> String {
    llvm_ir.replace(
        "  call void @aic_rt_env_set_args(i32 %argc, i8** %argv)\n",
        "",
    )
}

fn ensure_parent_dir(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

fn probe_toolchain(clang_bin: &str) -> anyhow::Result<ToolchainInfo> {
    let mut command = Command::new(clang_bin);
    command.arg("--version");
    let output = command
        .output()
        .with_context(|| format!("failed to execute {clang_bin} --version"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{clang_bin} --version failed: {}", stderr.trim());
    }
    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    let Some(major) = parse_llvm_major(&raw) else {
        anyhow::bail!(
            "could not parse LLVM major version from clang --version output; output was: {}",
            raw.lines().next().unwrap_or("<empty>")
        );
    };
    Ok(ToolchainInfo {
        clang_version: raw,
        llvm_major: major,
    })
}

fn resolve_clang_binary() -> anyhow::Result<String> {
    if let Ok(explicit) = std::env::var("AIC_CLANG") {
        if !explicit.trim().is_empty() && probe_toolchain(&explicit).is_ok() {
            return Ok(explicit);
        }
    }

    if probe_toolchain("clang").is_ok() {
        return Ok("clang".to_string());
    }

    if cfg!(target_os = "macos") {
        if probe_toolchain("/usr/bin/clang").is_ok() {
            return Ok("/usr/bin/clang".to_string());
        }
        if let Some(path) = resolve_xcrun_clang_path() {
            if probe_toolchain(&path).is_ok() {
                return Ok(path);
            }
        }
    }

    anyhow::bail!(
        "failed to locate a working clang toolchain. Set AIC_CLANG to a valid clang binary"
    );
}

fn resolve_xcrun_clang_path() -> Option<String> {
    let output = Command::new("xcrun")
        .arg("--find")
        .arg("clang")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

fn ensure_supported_toolchain(info: &ToolchainInfo) -> anyhow::Result<()> {
    let pinned_major = std::env::var("AIC_LLVM_PIN_MAJOR")
        .ok()
        .map(|value| {
            value.parse::<u32>().with_context(|| {
                format!("AIC_LLVM_PIN_MAJOR must be an integer major version, got '{value}'")
            })
        })
        .transpose()?;

    ensure_supported_toolchain_with_pin(info, pinned_major)
}

fn ensure_supported_toolchain_with_pin(
    info: &ToolchainInfo,
    pinned_major: Option<u32>,
) -> anyhow::Result<()> {
    if info.llvm_major < MIN_SUPPORTED_LLVM_MAJOR {
        anyhow::bail!(
            "unsupported LLVM/clang major version {}. Minimum supported major is {}. \
Install a newer clang or set AIC_LLVM_PIN_MAJOR to a supported major for reproducible builds.",
            info.llvm_major,
            MIN_SUPPORTED_LLVM_MAJOR
        );
    }

    if let Some(expected) = pinned_major {
        if info.llvm_major != expected {
            anyhow::bail!(
                "toolchain pin mismatch: AIC_LLVM_PIN_MAJOR={} but detected clang major {}. \
Install a matching clang or adjust AIC_LLVM_PIN_MAJOR.",
                expected,
                info.llvm_major
            );
        }
    }

    Ok(())
}

fn parse_llvm_major(version_output: &str) -> Option<u32> {
    for line in version_output.lines() {
        let marker = "version ";
        let Some(idx) = line.find(marker) else {
            continue;
        };
        let tail = &line[idx + marker.len()..];
        let digits = tail
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if digits.is_empty() {
            continue;
        }
        if let Ok(major) = digits.parse::<u32>() {
            return Some(major);
        }
    }
    None
}

fn run_checked_command(mut command: Command, tool: &str, action: &str) -> anyhow::Result<()> {
    let rendered = render_command(&command);
    let output = command.output().with_context(|| {
        format!("failed to execute {tool} while {action}; ensure `{tool}` is installed and in PATH")
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        format!("stderr: {stderr}")
    } else if !stdout.is_empty() {
        format!("stdout: {stdout}")
    } else {
        "no compiler output".to_string()
    };
    anyhow::bail!("{tool} failed while {action} ({rendered}); {detail}");
}

fn render_command(command: &Command) -> String {
    let mut out = command.get_program().to_string_lossy().to_string();
    for arg in command.get_args() {
        out.push(' ');
        out.push_str(&arg.to_string_lossy());
    }
    out
}

fn append_link_options(command: &mut Command, link: &LinkOptions) {
    for path in &link.search_paths {
        command.arg("-L").arg(path);
    }
    for object in &link.objects {
        command.arg(object);
    }
    for lib in &link.libs {
        command.arg(format!("-l{lib}"));
    }
}

fn append_target_triple(command: &mut Command, target_triple: Option<&str>) {
    if let Some(target) = target_triple {
        command.arg(format!("--target={target}"));
    }
}

fn target_is_wasm(target_triple: Option<&str>) -> bool {
    matches!(target_triple, Some(target) if target.starts_with("wasm32"))
}

fn target_supports_static_link(target_triple: Option<&str>) -> bool {
    match target_triple {
        Some(target) => target.contains("linux"),
        None => cfg!(target_os = "linux"),
    }
}

fn target_is_macos(target_triple: Option<&str>) -> bool {
    match target_triple {
        Some(target) => target.contains("apple-darwin"),
        None => cfg!(target_os = "macos"),
    }
}

fn normalize_macos_uuid_and_codesign(path: &Path) -> anyhow::Result<()> {
    remove_macos_signature(path)?;
    normalize_macos_uuid(path)?;
    codesign_macos_binary(path)?;
    Ok(())
}

fn normalize_macos_uuid(path: &Path) -> anyhow::Result<()> {
    const LC_UUID: u32 = 0x1b;
    const MH_MAGIC_64: u32 = 0xfeed_facf;
    const MACH_HEADER_64_SIZE: usize = 32;
    const NCMDS_OFFSET: usize = 16;
    const LOAD_COMMAND_SIZE: usize = 8;
    const UUID_BYTES: usize = 16;

    let mut bytes = fs::read(path)
        .with_context(|| format!("failed to read macOS artifact {}", path.display()))?;
    if bytes.len() < MACH_HEADER_64_SIZE {
        anyhow::bail!(
            "macOS artifact is too small to contain mach header: {}",
            path.display()
        );
    }
    let magic = read_u32_le(&bytes, 0).context("reading mach header magic")?;
    if magic != MH_MAGIC_64 {
        anyhow::bail!(
            "unsupported mach-o header magic {magic:#x} for {}",
            path.display()
        );
    }
    let ncmds = read_u32_le(&bytes, NCMDS_OFFSET).context("reading mach header ncmds")? as usize;
    let mut command_offset = MACH_HEADER_64_SIZE;
    let mut uuid_range: Option<std::ops::Range<usize>> = None;

    for _ in 0..ncmds {
        let cmd = read_u32_le(&bytes, command_offset).with_context(|| {
            format!(
                "reading load command id at offset {command_offset} in {}",
                path.display()
            )
        })?;
        let cmdsize = read_u32_le(&bytes, command_offset + 4).with_context(|| {
            format!(
                "reading load command size at offset {} in {}",
                command_offset + 4,
                path.display()
            )
        })? as usize;
        if cmdsize < LOAD_COMMAND_SIZE {
            anyhow::bail!(
                "invalid load command size {cmdsize} at offset {command_offset} in {}",
                path.display()
            );
        }
        let command_end = command_offset
            .checked_add(cmdsize)
            .context("load command size overflow")?;
        if command_end > bytes.len() {
            anyhow::bail!(
                "load command exceeds file bounds at offset {command_offset} in {}",
                path.display()
            );
        }

        if cmd == LC_UUID {
            if cmdsize < LOAD_COMMAND_SIZE + UUID_BYTES {
                anyhow::bail!(
                    "LC_UUID command is truncated at offset {command_offset} in {}",
                    path.display()
                );
            }
            let uuid_start = command_offset + LOAD_COMMAND_SIZE;
            let uuid_end = uuid_start + UUID_BYTES;
            uuid_range = Some(uuid_start..uuid_end);
            break;
        }

        command_offset = command_end;
    }

    let uuid_range = uuid_range.context(format!(
        "missing LC_UUID load command in macOS artifact {}",
        path.display()
    ))?;

    let mut normalized = bytes.clone();
    normalized[uuid_range.clone()].fill(0);
    let digest = Sha256::digest(&normalized);
    let mut deterministic_uuid = [0u8; UUID_BYTES];
    deterministic_uuid.copy_from_slice(&digest[..UUID_BYTES]);
    deterministic_uuid[6] = (deterministic_uuid[6] & 0x0f) | 0x40;
    deterministic_uuid[8] = (deterministic_uuid[8] & 0x3f) | 0x80;
    bytes[uuid_range].copy_from_slice(&deterministic_uuid);
    fs::write(path, bytes).with_context(|| {
        format!(
            "failed to write normalized macOS artifact {}",
            path.display()
        )
    })?;
    Ok(())
}

fn remove_macos_signature(path: &Path) -> anyhow::Result<()> {
    let codesign_bin = resolve_codesign_binary();
    let rendered = format!(
        "{} --remove-signature {}",
        codesign_bin.to_string_lossy(),
        path.display()
    );
    let output = Command::new(&codesign_bin)
        .arg("--remove-signature")
        .arg(path)
        .output()
        .with_context(|| format!("failed to execute {rendered}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.contains("code object is not signed at all") {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        format!("stderr: {stderr}")
    } else if !stdout.is_empty() {
        format!("stdout: {stdout}")
    } else {
        "no tool output".to_string()
    };
    anyhow::bail!("failed to remove signature ({rendered}); {detail}");
}

fn codesign_macos_binary(path: &Path) -> anyhow::Result<()> {
    let codesign_bin = resolve_codesign_binary();
    let rendered = format!(
        "{} --force --sign - {}",
        codesign_bin.to_string_lossy(),
        path.display()
    );
    let output = Command::new(&codesign_bin)
        .arg("--force")
        .arg("--sign")
        .arg("-")
        .arg(path)
        .output()
        .with_context(|| format!("failed to execute {rendered}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        format!("stderr: {stderr}")
    } else if !stdout.is_empty() {
        format!("stdout: {stdout}")
    } else {
        "no tool output".to_string()
    };
    anyhow::bail!("codesign failed ({rendered}); {detail}");
}

fn resolve_codesign_binary() -> PathBuf {
    if let Ok(path) = std::env::var("AIC_CODESIGN") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    let default = PathBuf::from("/usr/bin/codesign");
    if default.exists() {
        return default;
    }
    PathBuf::from("codesign")
}

fn read_u32_le(bytes: &[u8], offset: usize) -> anyhow::Result<u32> {
    let end = offset
        .checked_add(std::mem::size_of::<u32>())
        .context("offset overflow")?;
    let slice = bytes.get(offset..end).context("u32 read out of bounds")?;
    let mut word = [0u8; 4];
    word.copy_from_slice(slice);
    Ok(u32::from_le_bytes(word))
}

fn collect_type_templates(
    program: &ir::Program,
    type_map: &BTreeMap<ir::TypeId, String>,
) -> (
    BTreeMap<String, StructTemplate>,
    BTreeMap<String, EnumTemplate>,
    BTreeMap<String, Vec<VariantCtor>>,
) {
    let mut struct_templates = BTreeMap::new();
    let mut enum_templates = BTreeMap::new();

    for item in &program.items {
        match item {
            ir::Item::Struct(strukt) => {
                let fields = strukt
                    .fields
                    .iter()
                    .map(|field| {
                        let ty = type_map
                            .get(&field.ty)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string());
                        (field.name.clone(), ty)
                    })
                    .collect::<Vec<_>>();
                let field_defaults = strukt
                    .fields
                    .iter()
                    .filter_map(|field| {
                        field
                            .default_value
                            .as_ref()
                            .map(|expr| (field.name.clone(), expr.clone()))
                    })
                    .collect::<BTreeMap<_, _>>();
                struct_templates.insert(
                    strukt.name.clone(),
                    StructTemplate {
                        generics: strukt.generics.iter().map(|g| g.name.clone()).collect(),
                        fields,
                        field_defaults,
                    },
                );
            }
            ir::Item::Enum(enm) => {
                let variants = enm
                    .variants
                    .iter()
                    .map(|variant| {
                        let payload = variant.payload.and_then(|id| type_map.get(&id).cloned());
                        (variant.name.clone(), payload)
                    })
                    .collect::<Vec<_>>();
                enum_templates.insert(
                    enm.name.clone(),
                    EnumTemplate {
                        generics: enm.generics.iter().map(|g| g.name.clone()).collect(),
                        variants,
                    },
                );
            }
            _ => {}
        }
    }

    enum_templates
        .entry("Option".to_string())
        .or_insert_with(|| EnumTemplate {
            generics: vec!["T".to_string()],
            variants: vec![
                ("None".to_string(), None),
                ("Some".to_string(), Some("T".to_string())),
            ],
        });
    enum_templates
        .entry("Result".to_string())
        .or_insert_with(|| EnumTemplate {
            generics: vec!["T".to_string(), "E".to_string()],
            variants: vec![
                ("Ok".to_string(), Some("T".to_string())),
                ("Err".to_string(), Some("E".to_string())),
            ],
        });

    let mut variant_ctors: BTreeMap<String, Vec<VariantCtor>> = BTreeMap::new();
    for (enum_name, template) in &enum_templates {
        for (idx, (variant_name, _)) in template.variants.iter().enumerate() {
            variant_ctors
                .entry(variant_name.clone())
                .or_default()
                .push(VariantCtor {
                    enum_name: enum_name.clone(),
                    variant_index: idx,
                });
        }
    }
    for ctors in variant_ctors.values_mut() {
        ctors.sort_by(|a, b| {
            a.enum_name
                .cmp(&b.enum_name)
                .then(a.variant_index.cmp(&b.variant_index))
        });
    }

    (struct_templates, enum_templates, variant_ctors)
}

fn collect_internal_aliases_and_consts(
    program: &ir::Program,
    type_map: &BTreeMap<ir::TypeId, String>,
) -> (BTreeMap<String, AliasDef>, BTreeMap<String, ConstDef>) {
    let mut aliases = BTreeMap::new();
    let mut consts = BTreeMap::new();

    for item in &program.items {
        let ir::Item::Function(func) = item else {
            continue;
        };

        if let Some(alias_name) = decode_internal_type_alias(&func.name) {
            aliases.insert(
                alias_name.to_string(),
                AliasDef {
                    generics: func.generics.iter().map(|g| g.name.clone()).collect(),
                    target: type_map
                        .get(&func.ret_type)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string()),
                    span: func.span,
                },
            );
            continue;
        }

        if let Some(const_name) = decode_internal_const(&func.name) {
            consts.insert(
                const_name.to_string(),
                ConstDef {
                    declared_ty: type_map
                        .get(&func.ret_type)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string()),
                    init: func.body.tail.as_ref().map(|expr| (**expr).clone()),
                    span: func.span,
                },
            );
        }
    }

    (aliases, consts)
}

fn collect_drop_impl_methods(
    program: &ir::Program,
    type_map: &BTreeMap<ir::TypeId, String>,
) -> BTreeMap<String, String> {
    let mut drop_impls = BTreeMap::new();
    for item in &program.items {
        let ir::Item::Impl(impl_def) = item else {
            continue;
        };
        if impl_def.is_inherent || impl_def.trait_name != "Drop" {
            continue;
        }
        let Some(target_ty) = impl_def.trait_args.first() else {
            continue;
        };
        let Some(target_repr) = type_map.get(target_ty).cloned() else {
            continue;
        };
        let Some(drop_method) = impl_def
            .methods
            .iter()
            .find(|method| method_base_name(&method.name) == "drop")
        else {
            continue;
        };
        drop_impls
            .entry(target_repr)
            .or_insert_with(|| drop_method.name.clone());
    }
    drop_impls
}

fn collect_direct_calls_in_block(block: &ir::Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            ir::Stmt::Let { expr, .. }
            | ir::Stmt::Assign { expr, .. }
            | ir::Stmt::Expr { expr, .. }
            | ir::Stmt::Assert { expr, .. } => collect_direct_calls_in_expr(expr, out),
            ir::Stmt::Return { expr, .. } => {
                if let Some(expr) = expr {
                    collect_direct_calls_in_expr(expr, out);
                }
            }
        }
    }
    if let Some(tail) = &block.tail {
        collect_direct_calls_in_expr(tail, out);
    }
}

fn collect_direct_calls_in_expr(expr: &ir::Expr, out: &mut BTreeSet<String>) {
    match &expr.kind {
        ir::ExprKind::Call { callee, args, .. } => {
            if let Some(path) = extract_callee_path(callee) {
                if path.len() == 1 {
                    out.insert(path[0].clone());
                }
            }
            collect_direct_calls_in_expr(callee, out);
            for arg in args {
                collect_direct_calls_in_expr(arg, out);
            }
        }
        ir::ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                collect_direct_calls_in_expr(value, out);
            }
        }
        ir::ExprKind::FieldAccess { base, .. }
        | ir::ExprKind::Unary { expr: base, .. }
        | ir::ExprKind::Borrow { expr: base, .. }
        | ir::ExprKind::Await { expr: base }
        | ir::ExprKind::Try { expr: base } => collect_direct_calls_in_expr(base, out),
        ir::ExprKind::Binary { lhs, rhs, .. } => {
            collect_direct_calls_in_expr(lhs, out);
            collect_direct_calls_in_expr(rhs, out);
        }
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_direct_calls_in_expr(cond, out);
            collect_direct_calls_in_block(then_block, out);
            collect_direct_calls_in_block(else_block, out);
        }
        ir::ExprKind::Match { expr, arms } => {
            collect_direct_calls_in_expr(expr, out);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_direct_calls_in_expr(guard, out);
                }
                collect_direct_calls_in_expr(&arm.body, out);
            }
        }
        ir::ExprKind::While { cond, body } => {
            collect_direct_calls_in_expr(cond, out);
            collect_direct_calls_in_block(body, out);
        }
        ir::ExprKind::Loop { body } | ir::ExprKind::UnsafeBlock { block: body } => {
            collect_direct_calls_in_block(body, out);
        }
        ir::ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                collect_direct_calls_in_expr(expr, out);
            }
        }
        ir::ExprKind::Closure { body, .. } => collect_direct_calls_in_block(body, out),
        _ => {}
    }
}

fn collect_recursive_call_targets(program: &ir::Program) -> BTreeMap<String, BTreeSet<String>> {
    let mut direct_calls = BTreeMap::<String, BTreeSet<String>>::new();
    for item in &program.items {
        let ir::Item::Function(func) = item else {
            continue;
        };
        let mut calls = BTreeSet::new();
        collect_direct_calls_in_block(&func.body, &mut calls);
        direct_calls.insert(func.name.clone(), calls);
    }

    let mut recursive_targets = BTreeMap::<String, BTreeSet<String>>::new();
    for (caller, callees) in &direct_calls {
        for callee in callees {
            let is_recursive_edge = if callee == caller {
                true
            } else {
                direct_calls
                    .get(callee)
                    .map(|targets| targets.contains(caller))
                    .unwrap_or(false)
            };
            if is_recursive_edge {
                recursive_targets
                    .entry(caller.clone())
                    .or_default()
                    .insert(callee.clone());
            }
        }
    }
    recursive_targets
}

struct Generator<'a> {
    program: &'a ir::Program,
    file: &'a str,
    source_map: Option<SourceMap>,
    debug: Option<DebugState>,
    diagnostics: Vec<Diagnostic>,
    out: Vec<String>,
    globals: Vec<String>,
    string_counter: usize,
    temp_counter: usize,
    label_counter: usize,
    fn_sigs: BTreeMap<String, FnSig>,
    fn_llvm_names: BTreeMap<ir::SymbolId, String>,
    extern_decls: BTreeSet<String>,
    type_map: BTreeMap<ir::TypeId, String>,
    type_aliases: BTreeMap<String, AliasDef>,
    const_defs: BTreeMap<String, ConstDef>,
    const_values: BTreeMap<String, ConstValue>,
    const_failures: BTreeSet<String>,
    struct_templates: BTreeMap<String, StructTemplate>,
    enum_templates: BTreeMap<String, EnumTemplate>,
    variant_ctors: BTreeMap<String, Vec<VariantCtor>>,
    drop_impl_methods: BTreeMap<String, String>,
    generic_fn_instances: BTreeMap<String, Vec<GenericFnInstance>>,
    generic_fn_instances_by_symbol: BTreeMap<ir::SymbolId, Vec<GenericFnInstance>>,
    active_type_bindings: Option<BTreeMap<String, String>>,
    closure_counter: usize,
    deferred_fn_defs: Vec<Vec<String>>,
    fn_value_adapters: BTreeMap<String, String>,
    recursive_call_targets: BTreeMap<String, BTreeSet<String>>,
    dyn_traits: BTreeMap<String, DynTraitInfo>,
    dyn_vtable_globals: BTreeMap<String, String>,
    generated_dyn_wrappers: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct FnCtx {
    lines: Vec<String>,
    vars: Vec<BTreeMap<String, Local>>,
    drop_scopes: Vec<DropScope>,
    terminated: bool,
    current_label: String,
    ret_ty: LType,
    async_inner_ret: Option<LType>,
    debug_scope: Option<usize>,
    loop_stack: Vec<LoopFrame>,
    current_fn_name: String,
    current_fn_llvm_name: String,
    current_fn_sig: FnSig,
    tail_return_mode: bool,
}

#[derive(Debug, Clone)]
struct LoopFrame {
    break_label: String,
    continue_label: String,
    result_ty: Option<LType>,
    result_slot: Option<String>,
    scope_depth: usize,
}

#[derive(Debug, Clone, Default)]
struct DropScope {
    lexical_order: Vec<ir::SymbolId>,
    locals: BTreeMap<ir::SymbolId, DropSlot>,
}

#[derive(Debug, Clone)]
struct DropSlot {
    ty: LType,
    ptr: String,
    skip_resource_cleanup: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceDropAction {
    FsFileClose,
    MapClose,
    SetCloseInnerMap,
    NetTcpClose,
    NetTlsClose,
    BufferClose,
    ConcurrencyCloseChannel,
    ConcurrencyCloseMutex,
    ConcurrencyCloseRwLock,
    ConcurrencyArcRelease,
}

fn find_local(scopes: &[BTreeMap<String, Local>], name: &str) -> Option<Local> {
    for scope in scopes.iter().rev() {
        if let Some(local) = scope.get(name) {
            return Some(local.clone());
        }
    }
    None
}

fn lexical_block_drop_order(block: &ir::Block) -> Vec<ir::SymbolId> {
    block.lexical_drop_order()
}

fn extract_callee_path(callee: &ir::Expr) -> Option<Vec<String>> {
    fn walk(expr: &ir::Expr, out: &mut Vec<String>) -> bool {
        match &expr.kind {
            ir::ExprKind::Var(name) => {
                out.push(name.clone());
                true
            }
            ir::ExprKind::FieldAccess { base, field } => {
                if !walk(base, out) {
                    return false;
                }
                out.push(field.clone());
                true
            }
            _ => false,
        }
    }

    let mut out = Vec::new();
    if walk(callee, &mut out) {
        Some(out)
    } else {
        None
    }
}

fn is_declared_in_scopes(scopes: &[BTreeSet<String>], name: &str) -> bool {
    scopes.iter().rev().any(|scope| scope.contains(name))
}

fn collect_closure_captures_block(
    block: &ir::Block,
    scopes: &mut Vec<BTreeSet<String>>,
    captures: &mut BTreeSet<String>,
    known_functions: &BTreeSet<String>,
) {
    scopes.push(BTreeSet::new());
    for stmt in &block.stmts {
        match stmt {
            ir::Stmt::Let { name, expr, .. } => {
                collect_closure_captures_expr(expr, scopes, captures, known_functions);
                if let Some(scope) = scopes.last_mut() {
                    scope.insert(name.clone());
                }
            }
            ir::Stmt::Assign { target, expr, .. } => {
                if !is_declared_in_scopes(scopes, target) && !known_functions.contains(target) {
                    captures.insert(target.clone());
                }
                collect_closure_captures_expr(expr, scopes, captures, known_functions);
            }
            ir::Stmt::Expr { expr, .. } | ir::Stmt::Assert { expr, .. } => {
                collect_closure_captures_expr(expr, scopes, captures, known_functions);
            }
            ir::Stmt::Return {
                expr: Some(expr), ..
            } => collect_closure_captures_expr(expr, scopes, captures, known_functions),
            ir::Stmt::Return { expr: None, .. } => {}
        }
    }
    if let Some(tail) = &block.tail {
        collect_closure_captures_expr(tail, scopes, captures, known_functions);
    }
    scopes.pop();
}

fn collect_closure_captures_expr(
    expr: &ir::Expr,
    scopes: &mut Vec<BTreeSet<String>>,
    captures: &mut BTreeSet<String>,
    known_functions: &BTreeSet<String>,
) {
    match &expr.kind {
        ir::ExprKind::Var(name) => {
            if !is_declared_in_scopes(scopes, name) && !known_functions.contains(name) {
                captures.insert(name.clone());
            }
        }
        ir::ExprKind::Call { callee, args, .. } => {
            if let Some(path) = extract_callee_path(callee) {
                if path.len() == 1 {
                    let name = &path[0];
                    if is_declared_in_scopes(scopes, name) && !known_functions.contains(name) {
                        captures.insert(name.clone());
                    }
                } else {
                    collect_closure_captures_expr(callee, scopes, captures, known_functions);
                }
            } else {
                collect_closure_captures_expr(callee, scopes, captures, known_functions);
            }
            for arg in args {
                collect_closure_captures_expr(arg, scopes, captures, known_functions);
            }
        }
        ir::ExprKind::Closure { params, body, .. } => {
            let mut param_scope = BTreeSet::new();
            for param in params {
                param_scope.insert(param.name.clone());
            }
            scopes.push(param_scope);
            collect_closure_captures_block(body, scopes, captures, known_functions);
            scopes.pop();
        }
        ir::ExprKind::If {
            cond,
            then_block,
            else_block,
        } => {
            collect_closure_captures_expr(cond, scopes, captures, known_functions);
            collect_closure_captures_block(then_block, scopes, captures, known_functions);
            collect_closure_captures_block(else_block, scopes, captures, known_functions);
        }
        ir::ExprKind::While { cond, body } => {
            collect_closure_captures_expr(cond, scopes, captures, known_functions);
            collect_closure_captures_block(body, scopes, captures, known_functions);
        }
        ir::ExprKind::Loop { body } => {
            collect_closure_captures_block(body, scopes, captures, known_functions);
        }
        ir::ExprKind::Break { expr } => {
            if let Some(expr) = expr {
                collect_closure_captures_expr(expr, scopes, captures, known_functions);
            }
        }
        ir::ExprKind::Continue => {}
        ir::ExprKind::Match { expr, arms } => {
            collect_closure_captures_expr(expr, scopes, captures, known_functions);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_closure_captures_expr(guard, scopes, captures, known_functions);
                }
                collect_closure_captures_expr(&arm.body, scopes, captures, known_functions);
            }
        }
        ir::ExprKind::Binary { lhs, rhs, .. } => {
            collect_closure_captures_expr(lhs, scopes, captures, known_functions);
            collect_closure_captures_expr(rhs, scopes, captures, known_functions);
        }
        ir::ExprKind::Unary { expr, .. }
        | ir::ExprKind::Borrow { expr, .. }
        | ir::ExprKind::Await { expr }
        | ir::ExprKind::Try { expr } => {
            collect_closure_captures_expr(expr, scopes, captures, known_functions);
        }
        ir::ExprKind::UnsafeBlock { block } => {
            collect_closure_captures_block(block, scopes, captures, known_functions);
        }
        ir::ExprKind::StructInit { fields, .. } => {
            for (_, value, _) in fields {
                collect_closure_captures_expr(value, scopes, captures, known_functions);
            }
        }
        ir::ExprKind::FieldAccess { base, .. } => {
            collect_closure_captures_expr(base, scopes, captures, known_functions);
        }
        ir::ExprKind::Int(_)
        | ir::ExprKind::Float(_)
        | ir::ExprKind::Bool(_)
        | ir::ExprKind::Char(_)
        | ir::ExprKind::String(_)
        | ir::ExprKind::Unit => {}
    }
}

fn qualified_builtin_intrinsic(call_path: &[String]) -> Option<&'static str> {
    if call_path.len() < 2 {
        return None;
    }
    let name = call_path.last().map(String::as_str)?;
    let qualifier = call_path
        .get(call_path.len().saturating_sub(2))
        .map(String::as_str)?;
    match (qualifier, name) {
        ("io", "print_int") => Some("aic_io_print_int_intrinsic"),
        ("io", "print_str") => Some("aic_io_print_str_intrinsic"),
        ("io", "print_float") => Some("aic_io_print_float_intrinsic"),
        ("io", "read_line") => Some("aic_io_read_line_intrinsic"),
        ("io", "read_int") => Some("aic_io_read_int_intrinsic"),
        ("io", "read_char") => Some("aic_io_read_char_intrinsic"),
        ("io", "prompt") => Some("aic_io_prompt_intrinsic"),
        ("io", "eprint_str") => Some("aic_io_eprint_str_intrinsic"),
        ("io", "eprint_int") => Some("aic_io_eprint_int_intrinsic"),
        ("io", "println_str") => Some("aic_io_println_str_intrinsic"),
        ("io", "println_int") => Some("aic_io_println_int_intrinsic"),
        ("io", "print_bool") => Some("aic_io_print_bool_intrinsic"),
        ("io", "println_bool") => Some("aic_io_println_bool_intrinsic"),
        ("io", "flush_stdout") => Some("aic_io_flush_stdout_intrinsic"),
        ("io", "flush_stderr") => Some("aic_io_flush_stderr_intrinsic"),
        ("io", "panic") => Some("aic_io_panic_intrinsic"),
        ("log", "log") => Some("aic_log_emit_intrinsic"),
        ("log", "set_level") => Some("aic_log_set_level_intrinsic"),
        ("log", "set_json_output") => Some("aic_log_set_json_output_intrinsic"),
        ("time", "now_ms") => Some("aic_time_now_ms_intrinsic"),
        ("time", "monotonic_ms") => Some("aic_time_monotonic_ms_intrinsic"),
        ("time", "sleep_ms") => Some("aic_time_sleep_ms_intrinsic"),
        ("time", "parse_rfc3339") => Some("aic_time_parse_rfc3339_intrinsic"),
        ("time", "parse_iso8601") => Some("aic_time_parse_iso8601_intrinsic"),
        ("time", "format_rfc3339") => Some("aic_time_format_rfc3339_intrinsic"),
        ("time", "format_iso8601") => Some("aic_time_format_iso8601_intrinsic"),
        ("rand", "seed") => Some("aic_rand_seed_intrinsic"),
        ("rand", "random_int") => Some("aic_rand_int_intrinsic"),
        ("rand", "random_range") => Some("aic_rand_range_intrinsic"),
        ("conc", "spawn_task") => Some("aic_conc_spawn_intrinsic"),
        ("conc", "join_task") => Some("aic_conc_join_intrinsic"),
        ("conc", "timeout_task") => Some("aic_conc_join_timeout_intrinsic"),
        ("conc", "cancel_task") => Some("aic_conc_cancel_intrinsic"),
        ("conc", "spawn_fn") => Some("aic_conc_spawn_fn_intrinsic"),
        ("conc", "join_value") => Some("aic_conc_join_value_intrinsic"),
        ("conc", "spawn_group") => Some("aic_conc_spawn_group_intrinsic"),
        ("conc", "select_first") => Some("aic_conc_select_first_intrinsic"),
        ("conc", "channel_int") => Some("aic_conc_channel_int_intrinsic"),
        ("conc", "buffered_channel_int") => Some("aic_conc_channel_int_buffered_intrinsic"),
        ("conc", "channel_int_buffered") => Some("aic_conc_channel_int_buffered_intrinsic"),
        ("conc", "send_int") => Some("aic_conc_send_int_intrinsic"),
        ("conc", "try_send_int") => Some("aic_conc_try_send_int_intrinsic"),
        ("conc", "recv_int") => Some("aic_conc_recv_int_intrinsic"),
        ("conc", "try_recv_int") => Some("aic_conc_try_recv_int_intrinsic"),
        ("conc", "select_recv_int") => Some("aic_conc_select_recv_int_intrinsic"),
        ("conc", "close_channel") => Some("aic_conc_close_channel_intrinsic"),
        ("conc", "mutex_int") => Some("aic_conc_mutex_int_intrinsic"),
        ("conc", "lock_int") => Some("aic_conc_mutex_lock_intrinsic"),
        ("conc", "unlock_int") => Some("aic_conc_mutex_unlock_intrinsic"),
        ("conc", "close_mutex") => Some("aic_conc_mutex_close_intrinsic"),
        ("conc", "rwlock_int") => Some("aic_conc_rwlock_int_intrinsic"),
        ("conc", "read_lock_int") => Some("aic_conc_rwlock_read_intrinsic"),
        ("conc", "write_lock_int") => Some("aic_conc_rwlock_write_lock_intrinsic"),
        ("conc", "write_unlock_int") => Some("aic_conc_rwlock_write_unlock_intrinsic"),
        ("conc", "close_rwlock") => Some("aic_conc_rwlock_close_intrinsic"),
        ("conc", "arc_new") => Some("aic_conc_arc_new_intrinsic"),
        ("conc", "arc_clone") => Some("aic_conc_arc_clone_intrinsic"),
        ("conc", "arc_get") => Some("aic_conc_arc_get_intrinsic"),
        ("conc", "arc_strong_count") => Some("aic_conc_arc_strong_count_intrinsic"),
        ("conc", "atomic_int") => Some("aic_conc_atomic_int_intrinsic"),
        ("conc", "atomic_load") => Some("aic_conc_atomic_load_intrinsic"),
        ("conc", "atomic_store") => Some("aic_conc_atomic_store_intrinsic"),
        ("conc", "atomic_add") => Some("aic_conc_atomic_add_intrinsic"),
        ("conc", "atomic_sub") => Some("aic_conc_atomic_sub_intrinsic"),
        ("conc", "atomic_cas") => Some("aic_conc_atomic_cas_intrinsic"),
        ("conc", "atomic_bool") => Some("aic_conc_atomic_bool_intrinsic"),
        ("conc", "atomic_load_bool") => Some("aic_conc_atomic_load_bool_intrinsic"),
        ("conc", "atomic_store_bool") => Some("aic_conc_atomic_store_bool_intrinsic"),
        ("conc", "atomic_swap_bool") => Some("aic_conc_atomic_swap_bool_intrinsic"),
        ("conc", "thread_local") => Some("aic_conc_tl_new_intrinsic"),
        ("conc", "tl_get") => Some("aic_conc_tl_get_intrinsic"),
        ("conc", "tl_set") => Some("aic_conc_tl_set_intrinsic"),
        ("fs", "exists") => Some("aic_fs_exists_intrinsic"),
        ("fs", "read_text") => Some("aic_fs_read_text_intrinsic"),
        ("fs", "write_text") => Some("aic_fs_write_text_intrinsic"),
        ("fs", "append_text") => Some("aic_fs_append_text_intrinsic"),
        ("fs", "copy") => Some("aic_fs_copy_intrinsic"),
        ("fs", "move") => Some("aic_fs_move_intrinsic"),
        ("fs", "delete") => Some("aic_fs_delete_intrinsic"),
        ("fs", "metadata") => Some("aic_fs_metadata_intrinsic"),
        ("fs", "walk_dir") => Some("aic_fs_walk_dir_intrinsic"),
        ("fs", "temp_file") => Some("aic_fs_temp_file_intrinsic"),
        ("fs", "temp_dir") => Some("aic_fs_temp_dir_intrinsic"),
        ("fs", "read_bytes") => Some("aic_fs_read_bytes_intrinsic"),
        ("fs", "write_bytes") => Some("aic_fs_write_bytes_intrinsic"),
        ("fs", "append_bytes") => Some("aic_fs_append_bytes_intrinsic"),
        ("fs", "open_read") => Some("aic_fs_open_read_intrinsic"),
        ("fs", "open_write") => Some("aic_fs_open_write_intrinsic"),
        ("fs", "open_append") => Some("aic_fs_open_append_intrinsic"),
        ("fs", "file_read_line") => Some("aic_fs_file_read_line_intrinsic"),
        ("fs", "file_write_str") => Some("aic_fs_file_write_str_intrinsic"),
        ("fs", "file_close") => Some("aic_fs_file_close_intrinsic"),
        ("fs", "mkdir") => Some("aic_fs_mkdir_intrinsic"),
        ("fs", "mkdir_all") => Some("aic_fs_mkdir_all_intrinsic"),
        ("fs", "rmdir") => Some("aic_fs_rmdir_intrinsic"),
        ("fs", "list_dir") => Some("aic_fs_list_dir_intrinsic"),
        ("fs", "create_symlink") => Some("aic_fs_create_symlink_intrinsic"),
        ("fs", "read_symlink") => Some("aic_fs_read_symlink_intrinsic"),
        ("fs", "set_readonly") => Some("aic_fs_set_readonly_intrinsic"),
        ("env", "get") => Some("aic_env_get_intrinsic"),
        ("env", "set") => Some("aic_env_set_intrinsic"),
        ("env", "remove") => Some("aic_env_remove_intrinsic"),
        ("env", "cwd") => Some("aic_env_cwd_intrinsic"),
        ("env", "set_cwd") => Some("aic_env_set_cwd_intrinsic"),
        ("env", "args") => Some("aic_env_args_intrinsic"),
        ("env", "arg_count") => Some("aic_env_arg_count_intrinsic"),
        ("env", "arg_at") => Some("aic_env_arg_at_intrinsic"),
        ("env", "exit") => Some("aic_env_exit_intrinsic"),
        ("env", "all_vars") => Some("aic_env_all_vars_intrinsic"),
        ("env", "home_dir") => Some("aic_env_home_dir_intrinsic"),
        ("env", "temp_dir") => Some("aic_env_temp_dir_intrinsic"),
        ("env", "os_name") => Some("aic_env_os_name_intrinsic"),
        ("env", "arch") => Some("aic_env_arch_intrinsic"),
        ("map", "new_map") => Some("aic_map_new_intrinsic"),
        ("map", "close_map") => Some("aic_map_close_intrinsic"),
        ("map", "insert") => Some("aic_map_insert_intrinsic"),
        ("map", "get") => Some("aic_map_get_intrinsic"),
        ("map", "contains_key") => Some("aic_map_contains_key_intrinsic"),
        ("map", "remove") => Some("aic_map_remove_intrinsic"),
        ("map", "size") => Some("aic_map_size_intrinsic"),
        ("map", "keys") => Some("aic_map_keys_intrinsic"),
        ("map", "values") => Some("aic_map_values_intrinsic"),
        ("map", "entries") => Some("aic_map_entries_intrinsic"),
        ("vec", "new_vec") => Some("aic_vec_new_intrinsic"),
        ("vec", "new_vec_with_capacity") => Some("aic_vec_new_with_capacity_intrinsic"),
        ("vec", "vec_of") => Some("aic_vec_of_intrinsic"),
        ("vec", "get") => Some("aic_vec_get_intrinsic"),
        ("vec", "first") => Some("aic_vec_first_intrinsic"),
        ("vec", "last") => Some("aic_vec_last_intrinsic"),
        ("vec", "push") => Some("aic_vec_push_intrinsic"),
        ("vec", "pop") => Some("aic_vec_pop_intrinsic"),
        ("vec", "set") => Some("aic_vec_set_intrinsic"),
        ("vec", "insert") => Some("aic_vec_insert_intrinsic"),
        ("vec", "remove_at") => Some("aic_vec_remove_at_intrinsic"),
        ("vec", "contains") => Some("aic_vec_contains_intrinsic"),
        ("vec", "index_of") => Some("aic_vec_index_of_intrinsic"),
        ("vec", "reverse") => Some("aic_vec_reverse_intrinsic"),
        ("vec", "slice") => Some("aic_vec_slice_intrinsic"),
        ("vec", "append") => Some("aic_vec_append_intrinsic"),
        ("vec", "clear") => Some("aic_vec_clear_intrinsic"),
        ("vec", "reserve") => Some("aic_vec_reserve_intrinsic"),
        ("vec", "shrink_to_fit") => Some("aic_vec_shrink_to_fit_intrinsic"),
        ("string", "len") => Some("aic_string_len_intrinsic"),
        ("string", "contains") => Some("aic_string_contains_intrinsic"),
        ("string", "starts_with") => Some("aic_string_starts_with_intrinsic"),
        ("string", "ends_with") => Some("aic_string_ends_with_intrinsic"),
        ("string", "index_of") => Some("aic_string_index_of_intrinsic"),
        ("string", "last_index_of") => Some("aic_string_last_index_of_intrinsic"),
        ("string", "substring") => Some("aic_string_substring_intrinsic"),
        ("string", "char_at") => Some("aic_string_char_at_intrinsic"),
        ("string", "split") => Some("aic_string_split_intrinsic"),
        ("string", "split_first") => Some("aic_string_split_first_intrinsic"),
        ("string", "trim") => Some("aic_string_trim_intrinsic"),
        ("string", "trim_start") => Some("aic_string_trim_start_intrinsic"),
        ("string", "trim_end") => Some("aic_string_trim_end_intrinsic"),
        ("string", "to_upper") => Some("aic_string_to_upper_intrinsic"),
        ("string", "to_lower") => Some("aic_string_to_lower_intrinsic"),
        ("string", "replace") => Some("aic_string_replace_intrinsic"),
        ("string", "repeat") => Some("aic_string_repeat_intrinsic"),
        ("string", "parse_int") => Some("aic_string_parse_int_intrinsic"),
        ("string", "parse_float") => Some("aic_string_parse_float_intrinsic"),
        ("string", "int_to_string") => Some("aic_string_int_to_string_intrinsic"),
        ("string", "float_to_string") => Some("aic_string_float_to_string_intrinsic"),
        ("string", "bool_to_string") => Some("aic_string_bool_to_string_intrinsic"),
        ("string", "join") => Some("aic_string_join_intrinsic"),
        ("string", "format") => Some("aic_string_format_intrinsic"),
        ("char", "is_digit") => Some("aic_char_is_digit_intrinsic"),
        ("char", "is_alpha") => Some("aic_char_is_alpha_intrinsic"),
        ("char", "is_whitespace") => Some("aic_char_is_whitespace_intrinsic"),
        ("char", "char_to_int") => Some("aic_char_to_int_intrinsic"),
        ("char", "int_to_char") => Some("aic_char_int_to_char_intrinsic"),
        ("char", "chars") => Some("aic_char_chars_intrinsic"),
        ("char", "from_chars") => Some("aic_char_from_chars_intrinsic"),
        ("math", "abs") => Some("aic_math_abs_intrinsic"),
        ("math", "abs_float") => Some("aic_math_abs_float_intrinsic"),
        ("math", "min") => Some("aic_math_min_intrinsic"),
        ("math", "max") => Some("aic_math_max_intrinsic"),
        ("math", "pow") => Some("aic_math_pow_intrinsic"),
        ("math", "sqrt") => Some("aic_math_sqrt_intrinsic"),
        ("math", "floor") => Some("aic_math_floor_intrinsic"),
        ("math", "ceil") => Some("aic_math_ceil_intrinsic"),
        ("math", "round") => Some("aic_math_round_intrinsic"),
        ("math", "log") => Some("aic_math_log_intrinsic"),
        ("math", "sin") => Some("aic_math_sin_intrinsic"),
        ("math", "cos") => Some("aic_math_cos_intrinsic"),
        ("path", "join") => Some("aic_path_join_intrinsic"),
        ("path", "basename") => Some("aic_path_basename_intrinsic"),
        ("path", "dirname") => Some("aic_path_dirname_intrinsic"),
        ("path", "extension") => Some("aic_path_extension_intrinsic"),
        ("path", "is_abs") => Some("aic_path_is_abs_intrinsic"),
        ("proc", "spawn") => Some("aic_proc_spawn_intrinsic"),
        ("proc", "wait") => Some("aic_proc_wait_intrinsic"),
        ("proc", "kill") => Some("aic_proc_kill_intrinsic"),
        ("proc", "run") => Some("aic_proc_run_intrinsic"),
        ("proc", "pipe") => Some("aic_proc_pipe_intrinsic"),
        ("proc", "run_with") => Some("aic_proc_run_with_intrinsic"),
        ("proc", "is_running") => Some("aic_proc_is_running_intrinsic"),
        ("proc", "current_pid") => Some("aic_proc_current_pid_intrinsic"),
        ("proc", "run_timeout") => Some("aic_proc_run_timeout_intrinsic"),
        ("proc", "pipe_chain") => Some("aic_proc_pipe_chain_intrinsic"),
        ("net", "tcp_listen") => Some("aic_net_tcp_listen_intrinsic"),
        ("net", "tcp_local_addr") => Some("aic_net_tcp_local_addr_intrinsic"),
        ("net", "tcp_accept") => Some("aic_net_tcp_accept_intrinsic"),
        ("net", "tcp_connect") => Some("aic_net_tcp_connect_intrinsic"),
        ("net", "tcp_send") => Some("aic_net_tcp_send_intrinsic"),
        ("net", "tcp_send_timeout") => Some("aic_net_tcp_send_timeout_intrinsic"),
        ("net", "tcp_recv") => Some("aic_net_tcp_recv_intrinsic"),
        ("net", "tcp_close") => Some("aic_net_tcp_close_intrinsic"),
        ("net", "tcp_set_nodelay") => Some("aic_net_tcp_set_nodelay_intrinsic"),
        ("net", "tcp_get_nodelay") => Some("aic_net_tcp_get_nodelay_intrinsic"),
        ("net", "tcp_set_keepalive") => Some("aic_net_tcp_set_keepalive_intrinsic"),
        ("net", "tcp_get_keepalive") => Some("aic_net_tcp_get_keepalive_intrinsic"),
        ("net", "tcp_set_keepalive_idle_secs") => {
            Some("aic_net_tcp_set_keepalive_idle_secs_intrinsic")
        }
        ("net", "tcp_get_keepalive_idle_secs") => {
            Some("aic_net_tcp_get_keepalive_idle_secs_intrinsic")
        }
        ("net", "tcp_set_keepalive_interval_secs") => {
            Some("aic_net_tcp_set_keepalive_interval_secs_intrinsic")
        }
        ("net", "tcp_get_keepalive_interval_secs") => {
            Some("aic_net_tcp_get_keepalive_interval_secs_intrinsic")
        }
        ("net", "tcp_set_keepalive_count") => Some("aic_net_tcp_set_keepalive_count_intrinsic"),
        ("net", "tcp_get_keepalive_count") => Some("aic_net_tcp_get_keepalive_count_intrinsic"),
        ("net", "tcp_peer_addr") => Some("aic_net_tcp_peer_addr_intrinsic"),
        ("net", "tcp_shutdown") => Some("aic_net_tcp_shutdown_intrinsic"),
        ("net", "tcp_shutdown_read") => Some("aic_net_tcp_shutdown_read_intrinsic"),
        ("net", "tcp_shutdown_write") => Some("aic_net_tcp_shutdown_write_intrinsic"),
        ("net", "tcp_set_send_buffer_size") => Some("aic_net_tcp_set_send_buffer_size_intrinsic"),
        ("net", "tcp_get_send_buffer_size") => Some("aic_net_tcp_get_send_buffer_size_intrinsic"),
        ("net", "tcp_set_recv_buffer_size") => Some("aic_net_tcp_set_recv_buffer_size_intrinsic"),
        ("net", "tcp_get_recv_buffer_size") => Some("aic_net_tcp_get_recv_buffer_size_intrinsic"),
        ("net", "udp_bind") => Some("aic_net_udp_bind_intrinsic"),
        ("net", "udp_local_addr") => Some("aic_net_udp_local_addr_intrinsic"),
        ("net", "udp_send_to") => Some("aic_net_udp_send_to_intrinsic"),
        ("net", "udp_recv_from") => Some("aic_net_udp_recv_from_intrinsic"),
        ("net", "udp_close") => Some("aic_net_udp_close_intrinsic"),
        ("net", "dns_lookup") => Some("aic_net_dns_lookup_intrinsic"),
        ("net", "dns_lookup_all") => Some("aic_net_dns_lookup_all_intrinsic"),
        ("net", "dns_reverse") => Some("aic_net_dns_reverse_intrinsic"),
        ("net", "async_accept_submit") => Some("aic_net_async_accept_submit_intrinsic"),
        ("net", "async_tcp_send_submit") => Some("aic_net_async_send_submit_intrinsic"),
        ("net", "async_tcp_recv_submit") => Some("aic_net_async_recv_submit_intrinsic"),
        ("net", "async_wait_int") => Some("aic_net_async_wait_int_intrinsic"),
        ("net", "async_wait_string") => Some("aic_net_async_wait_string_intrinsic"),
        ("net", "async_cancel_int") => Some("aic_net_async_cancel_int_intrinsic"),
        ("net", "async_cancel_string") => Some("aic_net_async_cancel_string_intrinsic"),
        ("net", "async_shutdown") => Some("aic_net_async_shutdown_intrinsic"),
        ("tls", "tls_send_timeout") => Some("aic_tls_send_timeout_intrinsic"),
        ("tls", "tls_async_send_submit") => Some("aic_tls_async_send_submit_intrinsic"),
        ("tls", "tls_async_recv_submit") => Some("aic_tls_async_recv_submit_intrinsic"),
        ("tls", "tls_async_wait_int") => Some("aic_tls_async_wait_int_intrinsic"),
        ("tls", "tls_async_wait_string") => Some("aic_tls_async_wait_string_intrinsic"),
        ("tls", "tls_async_cancel_int") => Some("aic_tls_async_cancel_int_intrinsic"),
        ("tls", "tls_async_cancel_string") => Some("aic_tls_async_cancel_string_intrinsic"),
        ("tls", "tls_async_shutdown") => Some("aic_tls_async_shutdown_intrinsic"),
        ("buffer", "new_buffer") => Some("aic_buffer_new_intrinsic"),
        ("buffer", "new_growable_buffer") => Some("aic_buffer_new_growable_intrinsic"),
        ("buffer", "buffer_from_bytes") => Some("aic_buffer_from_bytes_intrinsic"),
        ("buffer", "buffer_to_bytes") => Some("aic_buffer_to_bytes_intrinsic"),
        ("buffer", "buf_position") => Some("aic_buffer_position_intrinsic"),
        ("buffer", "buf_remaining") => Some("aic_buffer_remaining_intrinsic"),
        ("buffer", "buf_seek") => Some("aic_buffer_seek_intrinsic"),
        ("buffer", "buf_reset") => Some("aic_buffer_reset_intrinsic"),
        ("buffer", "buf_close") => Some("aic_buffer_close_intrinsic"),
        ("buffer", "buf_read_u8") => Some("aic_buffer_read_u8_intrinsic"),
        ("buffer", "buf_read_i16_be") => Some("aic_buffer_read_i16_be_intrinsic"),
        ("buffer", "buf_read_u16_be") => Some("aic_buffer_read_u16_be_intrinsic"),
        ("buffer", "buf_read_i32_be") => Some("aic_buffer_read_i32_be_intrinsic"),
        ("buffer", "buf_read_u32_be") => Some("aic_buffer_read_u32_be_intrinsic"),
        ("buffer", "buf_read_i64_be") => Some("aic_buffer_read_i64_be_intrinsic"),
        ("buffer", "buf_read_u64_be") => Some("aic_buffer_read_u64_be_intrinsic"),
        ("buffer", "buf_read_i16_le") => Some("aic_buffer_read_i16_le_intrinsic"),
        ("buffer", "buf_read_u16_le") => Some("aic_buffer_read_u16_le_intrinsic"),
        ("buffer", "buf_read_i32_le") => Some("aic_buffer_read_i32_le_intrinsic"),
        ("buffer", "buf_read_u32_le") => Some("aic_buffer_read_u32_le_intrinsic"),
        ("buffer", "buf_read_i64_le") => Some("aic_buffer_read_i64_le_intrinsic"),
        ("buffer", "buf_read_u64_le") => Some("aic_buffer_read_u64_le_intrinsic"),
        ("buffer", "buf_read_bytes") => Some("aic_buffer_read_bytes_intrinsic"),
        ("buffer", "buf_read_cstring") => Some("aic_buffer_read_cstring_intrinsic"),
        ("buffer", "buf_read_length_prefixed") => Some("aic_buffer_read_length_prefixed_intrinsic"),
        ("buffer", "buf_write_u8") => Some("aic_buffer_write_u8_intrinsic"),
        ("buffer", "buf_write_i16_be") => Some("aic_buffer_write_i16_be_intrinsic"),
        ("buffer", "buf_write_u16_be") => Some("aic_buffer_write_u16_be_intrinsic"),
        ("buffer", "buf_write_i32_be") => Some("aic_buffer_write_i32_be_intrinsic"),
        ("buffer", "buf_write_u32_be") => Some("aic_buffer_write_u32_be_intrinsic"),
        ("buffer", "buf_write_i64_be") => Some("aic_buffer_write_i64_be_intrinsic"),
        ("buffer", "buf_write_u64_be") => Some("aic_buffer_write_u64_be_intrinsic"),
        ("buffer", "buf_write_i16_le") => Some("aic_buffer_write_i16_le_intrinsic"),
        ("buffer", "buf_write_u16_le") => Some("aic_buffer_write_u16_le_intrinsic"),
        ("buffer", "buf_write_i32_le") => Some("aic_buffer_write_i32_le_intrinsic"),
        ("buffer", "buf_write_u32_le") => Some("aic_buffer_write_u32_le_intrinsic"),
        ("buffer", "buf_write_i64_le") => Some("aic_buffer_write_i64_le_intrinsic"),
        ("buffer", "buf_write_u64_le") => Some("aic_buffer_write_u64_le_intrinsic"),
        ("buffer", "buf_write_bytes") => Some("aic_buffer_write_bytes_intrinsic"),
        ("buffer", "buf_write_cstring") => Some("aic_buffer_write_cstring_intrinsic"),
        ("buffer", "buf_write_string_prefixed") => {
            Some("aic_buffer_write_string_prefixed_intrinsic")
        }
        ("buffer", "buf_patch_u16_be") => Some("aic_buffer_patch_u16_be_intrinsic"),
        ("buffer", "buf_patch_u32_be") => Some("aic_buffer_patch_u32_be_intrinsic"),
        ("buffer", "buf_patch_u64_be") => Some("aic_buffer_patch_u64_be_intrinsic"),
        ("buffer", "buf_patch_u16_le") => Some("aic_buffer_patch_u16_le_intrinsic"),
        ("buffer", "buf_patch_u32_le") => Some("aic_buffer_patch_u32_le_intrinsic"),
        ("buffer", "buf_patch_u64_le") => Some("aic_buffer_patch_u64_le_intrinsic"),
        ("crypto", "md5") => Some("aic_crypto_md5_intrinsic"),
        ("crypto", "md5_bytes") => Some("aic_crypto_md5_intrinsic"),
        ("crypto", "sha256") => Some("aic_crypto_sha256_intrinsic"),
        ("crypto", "sha256_raw") => Some("aic_crypto_sha256_raw_intrinsic"),
        ("crypto", "hmac_sha256") => Some("aic_crypto_hmac_sha256_intrinsic"),
        ("crypto", "hmac_sha256_raw") => Some("aic_crypto_hmac_sha256_raw_intrinsic"),
        ("crypto", "pbkdf2_sha256") => Some("aic_crypto_pbkdf2_sha256_intrinsic"),
        ("crypto", "hex_encode") => Some("aic_crypto_hex_encode_intrinsic"),
        ("crypto", "hex_decode") => Some("aic_crypto_hex_decode_intrinsic"),
        ("crypto", "base64_encode") => Some("aic_crypto_base64_encode_intrinsic"),
        ("crypto", "base64_decode") => Some("aic_crypto_base64_decode_intrinsic"),
        ("crypto", "random_bytes") => Some("aic_crypto_random_bytes_intrinsic"),
        ("crypto", "secure_eq") => Some("aic_crypto_secure_eq_intrinsic"),
        ("url", "parse") => Some("aic_url_parse_intrinsic"),
        ("url", "normalize") => Some("aic_url_normalize_intrinsic"),
        ("url", "net_addr") => Some("aic_url_net_addr_intrinsic"),
        ("http", "parse_method") => Some("aic_http_parse_method_intrinsic"),
        ("http", "method_name") => Some("aic_http_method_name_intrinsic"),
        ("http", "status_reason") => Some("aic_http_status_reason_intrinsic"),
        ("http", "validate_header") => Some("aic_http_validate_header_intrinsic"),
        ("http", "validate_target") => Some("aic_http_validate_target_intrinsic"),
        ("http", "header") => Some("aic_http_header_intrinsic"),
        ("http", "request") => Some("aic_http_request_intrinsic"),
        ("http", "response") => Some("aic_http_response_intrinsic"),
        ("http_server", "listen") => Some("aic_http_server_listen_intrinsic"),
        ("http_server", "accept") => Some("aic_http_server_accept_intrinsic"),
        ("http_server", "read_request") => Some("aic_http_server_read_request_intrinsic"),
        ("http_server", "write_response") => Some("aic_http_server_write_response_intrinsic"),
        ("http_server", "close") => Some("aic_http_server_close_intrinsic"),
        ("http_server", "text_response") => Some("aic_http_server_text_response_intrinsic"),
        ("http_server", "json_response") => Some("aic_http_server_json_response_intrinsic"),
        ("http_server", "header") => Some("aic_http_server_header_intrinsic"),
        ("router", "new_router") => Some("aic_router_new_intrinsic"),
        ("router", "add") => Some("aic_router_add_intrinsic"),
        ("router", "match_route") => Some("aic_router_match_intrinsic"),
        ("json", "parse") => Some("aic_json_parse_intrinsic"),
        ("json", "stringify") => Some("aic_json_stringify_intrinsic"),
        ("json", "encode_int") => Some("aic_json_encode_int_intrinsic"),
        ("json", "encode_float") => Some("aic_json_encode_float_intrinsic"),
        ("json", "encode_bool") => Some("aic_json_encode_bool_intrinsic"),
        ("json", "encode_string") => Some("aic_json_encode_string_intrinsic"),
        ("json", "encode_null") => Some("aic_json_encode_null_intrinsic"),
        ("json", "encode") => Some("aic_json_serde_encode_intrinsic"),
        ("json", "decode_int") => Some("aic_json_decode_int_intrinsic"),
        ("json", "decode_float") => Some("aic_json_decode_float_intrinsic"),
        ("json", "decode_bool") => Some("aic_json_decode_bool_intrinsic"),
        ("json", "decode_string") => Some("aic_json_decode_string_intrinsic"),
        ("json", "decode_with") => Some("aic_json_serde_decode_intrinsic"),
        ("json", "schema") => Some("aic_json_serde_schema_intrinsic"),
        ("json", "object_empty") => Some("aic_json_object_empty_intrinsic"),
        ("json", "object_set") => Some("aic_json_object_set_intrinsic"),
        ("json", "object_get") => Some("aic_json_object_get_intrinsic"),
        ("json", "kind") => Some("aic_json_kind_intrinsic"),
        ("regex", "compile_with_flags") => Some("aic_regex_compile_intrinsic"),
        ("regex", "is_match") => Some("aic_regex_is_match_intrinsic"),
        ("regex", "find") => Some("aic_regex_find_intrinsic"),
        ("regex", "captures") => Some("aic_regex_captures_intrinsic"),
        ("regex", "replace") => Some("aic_regex_replace_intrinsic"),
        _ => None,
    }
}

fn coerce_repr(value: &Value, expected: &LType) -> String {
    if value.ty == *expected {
        return value
            .repr
            .clone()
            .unwrap_or_else(|| default_value(expected));
    }
    default_value(expected)
}

fn llvm_float_literal(value: f64) -> String {
    format!("0x{:016X}", value.to_bits())
}

fn llvm_type(ty: &LType) -> String {
    match ty {
        LType::Int => "i64".to_string(),
        LType::Float => "double".to_string(),
        LType::Bool => "i1".to_string(),
        LType::Char => "i32".to_string(),
        LType::Unit => "void".to_string(),
        LType::String => "{ i8*, i64, i64 }".to_string(),
        LType::Fn(_) => "{ i8*, i8* }".to_string(),
        LType::DynTrait(_) => "{ i8*, i8* }".to_string(),
        LType::Async(inner) => {
            if matches!(&**inner, LType::Unit) {
                "{ i1 }".to_string()
            } else {
                format!("{{ i1, {} }}", llvm_type(inner))
            }
        }
        LType::Struct(layout) => {
            if layout.fields.is_empty() {
                "{}".to_string()
            } else {
                let fields = layout
                    .fields
                    .iter()
                    .map(|field| llvm_type(&field.ty))
                    .collect::<Vec<_>>();
                format!("{{ {} }}", fields.join(", "))
            }
        }
        LType::Enum(layout) => {
            let mut parts = Vec::new();
            parts.push("i32".to_string());
            for variant in &layout.variants {
                parts.push(match &variant.payload {
                    Some(payload) => {
                        if *payload == LType::Unit {
                            "i8".to_string()
                        } else {
                            llvm_type(payload)
                        }
                    }
                    None => "i8".to_string(),
                });
            }
            format!("{{ {} }}", parts.join(", "))
        }
    }
}

fn default_value(ty: &LType) -> String {
    match ty {
        LType::Int => "0".to_string(),
        LType::Float => llvm_float_literal(0.0_f64),
        LType::Bool => "0".to_string(),
        LType::Char => "0".to_string(),
        LType::Unit => String::new(),
        LType::String => "{ i8* null, i64 0, i64 0 }".to_string(),
        LType::Fn(_) => "{ i8* null, i8* null }".to_string(),
        LType::DynTrait(_) => "{ i8* null, i8* null }".to_string(),
        LType::Async(inner) => {
            if matches!(&**inner, LType::Unit) {
                "{ i1 0 }".to_string()
            } else {
                format!("{{ i1 0, {} {} }}", llvm_type(inner), default_value(inner))
            }
        }
        LType::Struct(layout) => {
            if layout.fields.is_empty() {
                "{}".to_string()
            } else {
                let fields = layout
                    .fields
                    .iter()
                    .map(|field| format!("{} {}", llvm_type(&field.ty), default_value(&field.ty)))
                    .collect::<Vec<_>>();
                format!("{{ {} }}", fields.join(", "))
            }
        }
        LType::Enum(layout) => {
            let mut fields = vec!["i32 0".to_string()];
            for variant in &layout.variants {
                match &variant.payload {
                    Some(payload) => {
                        if *payload == LType::Unit {
                            fields.push("i8 0".to_string());
                        } else {
                            fields.push(format!(
                                "{} {}",
                                llvm_type(payload),
                                default_value(payload)
                            ));
                        }
                    }
                    None => fields.push("i8 0".to_string()),
                }
            }
            format!("{{ {} }}", fields.join(", "))
        }
    }
}

fn type_has_runtime_drop(ty: &LType) -> bool {
    matches!(ty, LType::String | LType::Struct(_) | LType::Enum(_))
}

fn resource_drop_action_for_type(ty: &LType) -> Option<ResourceDropAction> {
    let LType::Struct(layout) = ty else {
        return None;
    };
    let base = base_type_name(&layout.repr);
    let short = base.rsplit('.').next().unwrap_or(base);
    match short {
        "FileHandle" => Some(ResourceDropAction::FsFileClose),
        "Map" => Some(ResourceDropAction::MapClose),
        "Set" => Some(ResourceDropAction::SetCloseInnerMap),
        "TcpReader" => Some(ResourceDropAction::NetTcpClose),
        "TlsStream" => Some(ResourceDropAction::NetTlsClose),
        "ByteBuffer" => Some(ResourceDropAction::BufferClose),
        "IntChannel" => Some(ResourceDropAction::ConcurrencyCloseChannel),
        "IntMutex" => Some(ResourceDropAction::ConcurrencyCloseMutex),
        "IntRwLock" => Some(ResourceDropAction::ConcurrencyCloseRwLock),
        "Arc" => Some(ResourceDropAction::ConcurrencyArcRelease),
        _ => None,
    }
}

fn resource_drop_runtime_fn(action: ResourceDropAction) -> &'static str {
    match action {
        ResourceDropAction::FsFileClose => "aic_rt_fs_file_close",
        ResourceDropAction::MapClose | ResourceDropAction::SetCloseInnerMap => "aic_rt_map_close",
        ResourceDropAction::NetTcpClose => "aic_rt_net_tcp_close",
        ResourceDropAction::NetTlsClose => "aic_rt_tls_close",
        ResourceDropAction::BufferClose => "aic_rt_buffer_close",
        ResourceDropAction::ConcurrencyCloseChannel => "aic_rt_conc_close_channel",
        ResourceDropAction::ConcurrencyCloseMutex => "aic_rt_conc_mutex_close",
        ResourceDropAction::ConcurrencyCloseRwLock => "aic_rt_conc_rwlock_close",
        ResourceDropAction::ConcurrencyArcRelease => "aic_rt_conc_arc_release",
    }
}

fn render_type(ty: &LType) -> String {
    match ty {
        LType::Int => "Int".to_string(),
        LType::Float => "Float".to_string(),
        LType::Bool => "Bool".to_string(),
        LType::Char => "Char".to_string(),
        LType::Unit => "()".to_string(),
        LType::String => "String".to_string(),
        LType::Fn(layout) => layout.repr.clone(),
        LType::DynTrait(trait_name) => format!("dyn {}", trait_name),
        LType::Struct(layout) => layout.repr.clone(),
        LType::Enum(layout) => layout.repr.clone(),
        LType::Async(inner) => format!("Async[{}]", render_type(inner)),
    }
}

fn render_applied_type(base: &str, args: &[LType]) -> String {
    let parts = args.iter().map(render_type).collect::<Vec<_>>();
    render_applied_type_from_parts(base, &parts)
}

fn render_applied_type_from_parts(base: &str, args: &[String]) -> String {
    if args.is_empty() {
        base.to_string()
    } else {
        format!("{base}[{}]", args.join(", "))
    }
}

fn dyn_wrapper_function_type(method: &DynTraitMethodInfo) -> String {
    let mut params = vec!["i8*".to_string()];
    params.extend(method.params.iter().map(llvm_type));
    format!("{} ({})", llvm_type(&method.ret), params.join(", "))
}

fn type_uses_self_repr(ty: &str) -> bool {
    if ty.trim() == "Self" {
        return true;
    }
    extract_generic_args(ty)
        .map(|args| args.iter().any(|arg| type_uses_self_repr(arg)))
        .unwrap_or(false)
}

fn method_base_name(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

fn infer_generic_bindings(
    expected: &str,
    found: &str,
    generic_params: &[String],
    bindings: &mut BTreeMap<String, String>,
) -> bool {
    let expected = expected.trim();
    let found = found.trim();
    if generic_params.iter().any(|g| g == expected) {
        if let Some(existing) = bindings.get(expected) {
            return existing == found;
        }
        bindings.insert(expected.to_string(), found.to_string());
        return true;
    }

    let expected_args = extract_generic_args(expected).unwrap_or_default();
    let found_args = extract_generic_args(found).unwrap_or_default();
    if expected_args.is_empty() || found_args.is_empty() {
        return expected == found;
    }
    if base_type_name(expected) != base_type_name(found) || expected_args.len() != found_args.len()
    {
        return false;
    }
    for (exp, got) in expected_args.iter().zip(found_args.iter()) {
        if !infer_generic_bindings(exp, got, generic_params, bindings) {
            return false;
        }
    }
    true
}

fn substitute_type_vars(ty: &str, bindings: &BTreeMap<String, String>) -> String {
    let ty = ty.trim();
    if let Some(bound) = bindings.get(ty) {
        return bound.clone();
    }

    let Some(args) = extract_generic_args(ty) else {
        return ty.to_string();
    };
    let base = base_type_name(ty);
    let mapped = args
        .iter()
        .map(|arg| substitute_type_vars(arg, bindings))
        .collect::<Vec<_>>();
    render_applied_type_from_parts(base, &mapped)
}

fn base_type_name(ty: &str) -> &str {
    let ty = ty.trim();
    match ty.find('[') {
        Some(idx) => ty[..idx].trim(),
        None => ty,
    }
}

fn extract_generic_args(ty: &str) -> Option<Vec<String>> {
    let ty = ty.trim();
    let start = ty.find('[')?;
    let end = ty.rfind(']')?;
    if end <= start {
        return None;
    }
    if !ty[end + 1..].trim().is_empty() {
        return None;
    }
    Some(split_top_level(&ty[start + 1..end]))
}

fn split_top_level(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in text.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(text[start..idx].trim().to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        parts.push(tail.to_string());
    }
    parts
}

fn unary_op_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
        UnaryOp::BitNot => "~",
    }
}

fn binary_op_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Ushr => ">>>",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

fn const_value_name(value: &ConstValue) -> &'static str {
    match value {
        ConstValue::Int(_) => "Int",
        ConstValue::Float(_) => "Float",
        ConstValue::Bool(_) => "Bool",
        ConstValue::Char(_) => "Char",
        ConstValue::Unit => "()",
        ConstValue::String(_) => "String",
    }
}

fn mangle_generic_instantiation(kind_tag: &str, name: &str, type_args: &[String]) -> String {
    let mut out = String::new();
    out.push_str(kind_tag);
    out.push('_');
    out.push_str(&mangle_generic_component(name));
    for arg in type_args {
        out.push('_');
        out.push_str(&mangle_generic_component(arg));
    }
    out
}

fn mangle_generic_component(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '_' => out.push(ch),
            '[' => out.push_str("_lb_"),
            ']' => out.push_str("_rb_"),
            ',' => out.push_str("_c_"),
            ' ' => {}
            other => out.push_str(&format!("_x{:02X}_", other as u32)),
        }
    }
    out
}

fn mangle(name: &str) -> String {
    let mut out = String::from("aic_");
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

fn escape_llvm_string(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'\\' => out.push_str("\\5C"),
            b'"' => out.push_str("\\22"),
            b'\n' => out.push_str("\\0A"),
            b'\t' => out.push_str("\\09"),
            32..=126 => out.push(byte as char),
            _ => out.push_str(&format!("\\{:02X}", byte)),
        }
    }
    out
}

fn escape_c_string_bytes(text: &str) -> (String, usize) {
    let mut out = String::new();
    let mut len = 0usize;
    for b in text.bytes() {
        len += 1;
        match b {
            b'\\' => out.push_str("\\5C"),
            b'"' => out.push_str("\\22"),
            b'\n' => out.push_str("\\0A"),
            b'\t' => out.push_str("\\09"),
            32..=126 => out.push(b as char),
            _ => out.push_str(&format!("\\{:02X}", b)),
        }
    }
    out.push_str("\\00");
    len += 1;
    (out, len)
}

fn json_escape_string(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04X}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}
