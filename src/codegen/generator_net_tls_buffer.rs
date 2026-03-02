use super::*;

impl<'a> Generator<'a> {
    pub(super) fn gen_net_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "tcp_listen" | "aic_net_tcp_listen_intrinsic" => "tcp_listen",
            "tcp_local_addr" | "aic_net_tcp_local_addr_intrinsic" => "tcp_local_addr",
            "tcp_accept" | "aic_net_tcp_accept_intrinsic" => "tcp_accept",
            "tcp_connect" | "aic_net_tcp_connect_intrinsic" => "tcp_connect",
            "tcp_send" | "aic_net_tcp_send_intrinsic" => "tcp_send",
            "tcp_send_timeout" | "aic_net_tcp_send_timeout_intrinsic" => "tcp_send_timeout",
            "aic_net_tcp_recv_intrinsic" => "tcp_recv",
            "tcp_close" | "aic_net_tcp_close_intrinsic" => "tcp_close",
            "tcp_set_nodelay" | "aic_net_tcp_set_nodelay_intrinsic" => "tcp_set_nodelay",
            "tcp_get_nodelay" | "aic_net_tcp_get_nodelay_intrinsic" => "tcp_get_nodelay",
            "tcp_set_keepalive" | "aic_net_tcp_set_keepalive_intrinsic" => "tcp_set_keepalive",
            "tcp_get_keepalive" | "aic_net_tcp_get_keepalive_intrinsic" => "tcp_get_keepalive",
            "tcp_set_keepalive_idle_secs" | "aic_net_tcp_set_keepalive_idle_secs_intrinsic" => {
                "tcp_set_keepalive_idle_secs"
            }
            "tcp_get_keepalive_idle_secs" | "aic_net_tcp_get_keepalive_idle_secs_intrinsic" => {
                "tcp_get_keepalive_idle_secs"
            }
            "tcp_set_keepalive_interval_secs"
            | "aic_net_tcp_set_keepalive_interval_secs_intrinsic" => {
                "tcp_set_keepalive_interval_secs"
            }
            "tcp_get_keepalive_interval_secs"
            | "aic_net_tcp_get_keepalive_interval_secs_intrinsic" => {
                "tcp_get_keepalive_interval_secs"
            }
            "tcp_set_keepalive_count" | "aic_net_tcp_set_keepalive_count_intrinsic" => {
                "tcp_set_keepalive_count"
            }
            "tcp_get_keepalive_count" | "aic_net_tcp_get_keepalive_count_intrinsic" => {
                "tcp_get_keepalive_count"
            }
            "tcp_peer_addr" | "aic_net_tcp_peer_addr_intrinsic" => "tcp_peer_addr",
            "tcp_shutdown" | "aic_net_tcp_shutdown_intrinsic" => "tcp_shutdown",
            "tcp_shutdown_read" | "aic_net_tcp_shutdown_read_intrinsic" => "tcp_shutdown_read",
            "tcp_shutdown_write" | "aic_net_tcp_shutdown_write_intrinsic" => "tcp_shutdown_write",
            "tcp_set_send_buffer_size" | "aic_net_tcp_set_send_buffer_size_intrinsic" => {
                "tcp_set_send_buffer_size"
            }
            "tcp_get_send_buffer_size" | "aic_net_tcp_get_send_buffer_size_intrinsic" => {
                "tcp_get_send_buffer_size"
            }
            "tcp_set_recv_buffer_size" | "aic_net_tcp_set_recv_buffer_size_intrinsic" => {
                "tcp_set_recv_buffer_size"
            }
            "tcp_get_recv_buffer_size" | "aic_net_tcp_get_recv_buffer_size_intrinsic" => {
                "tcp_get_recv_buffer_size"
            }
            "udp_bind" | "aic_net_udp_bind_intrinsic" => "udp_bind",
            "udp_local_addr" | "aic_net_udp_local_addr_intrinsic" => "udp_local_addr",
            "udp_send_to" | "aic_net_udp_send_to_intrinsic" => "udp_send_to",
            "udp_recv_from" | "aic_net_udp_recv_from_intrinsic" => "udp_recv_from",
            "udp_close" | "aic_net_udp_close_intrinsic" => "udp_close",
            "dns_lookup" | "aic_net_dns_lookup_intrinsic" => "dns_lookup",
            "dns_lookup_all" | "aic_net_dns_lookup_all_intrinsic" => "dns_lookup_all",
            "dns_reverse" | "aic_net_dns_reverse_intrinsic" => "dns_reverse",
            "async_accept_submit" | "aic_net_async_accept_submit_intrinsic" => {
                "async_accept_submit"
            }
            "async_tcp_send_submit" | "aic_net_async_send_submit_intrinsic" => {
                "async_tcp_send_submit"
            }
            "async_tcp_recv_submit" | "aic_net_async_recv_submit_intrinsic" => {
                "async_tcp_recv_submit"
            }
            "async_wait_int" | "aic_net_async_wait_int_intrinsic" => "async_wait_int",
            "aic_net_async_wait_string_intrinsic" => "async_wait_string",
            "aic_net_async_cancel_int_intrinsic" => "async_cancel_int",
            "aic_net_async_cancel_string_intrinsic" => "async_cancel_string",
            "async_shutdown" | "aic_net_async_shutdown_intrinsic" => "async_shutdown",
            _ => return None,
        };

        match canonical {
            "tcp_listen" if self.sig_matches_shape(name, &["String"], "Result[Int, NetError]") => {
                Some(self.gen_net_listen_or_bind_call(
                    name,
                    "aic_rt_net_tcp_listen",
                    args,
                    span,
                    fctx,
                ))
            }
            "udp_bind" if self.sig_matches_shape(name, &["String"], "Result[Int, NetError]") => {
                Some(self.gen_net_listen_or_bind_call(
                    name,
                    "aic_rt_net_udp_bind",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_local_addr"
                if self.sig_matches_shape(name, &["Int"], "Result[String, NetError]") =>
            {
                Some(self.gen_net_local_addr_call(
                    name,
                    "aic_rt_net_tcp_local_addr",
                    args,
                    span,
                    fctx,
                ))
            }
            "udp_local_addr"
                if self.sig_matches_shape(name, &["Int"], "Result[String, NetError]") =>
            {
                Some(self.gen_net_local_addr_call(
                    name,
                    "aic_rt_net_udp_local_addr",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_peer_addr"
                if self.sig_matches_shape(name, &["Int"], "Result[String, NetError]") =>
            {
                Some(self.gen_net_local_addr_call(
                    name,
                    "aic_rt_net_tcp_peer_addr",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_accept"
                if self.sig_matches_shape(name, &["Int", "Int"], "Result[Int, NetError]") =>
            {
                Some(self.gen_net_tcp_accept_call(name, args, span, fctx))
            }
            "tcp_connect"
                if self.sig_matches_shape(name, &["String", "Int"], "Result[Int, NetError]") =>
            {
                Some(self.gen_net_tcp_connect_call(name, args, span, fctx))
            }
            "tcp_send"
                if self.sig_matches_shape(name, &["Int", "Bytes"], "Result[Int, NetError]")
                    || self.sig_matches_shape(
                        name,
                        &["Int", "String"],
                        "Result[Int, NetError]",
                    ) =>
            {
                Some(self.gen_net_tcp_send_call(name, args, span, fctx))
            }
            "tcp_send_timeout"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Bytes", "Int"],
                    "Result[Int, NetError]",
                ) || self.sig_matches_shape(
                    name,
                    &["Int", "String", "Int"],
                    "Result[Int, NetError]",
                ) =>
            {
                Some(self.gen_net_tcp_send_timeout_call(name, args, span, fctx))
            }
            "tcp_recv"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[Bytes, NetError]",
                ) || self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[String, NetError]",
                ) =>
            {
                Some(self.gen_net_tcp_recv_call(name, args, span, fctx))
            }
            "tcp_close" if self.sig_matches_shape(name, &["Int"], "Result[Bool, NetError]") => {
                Some(self.gen_net_close_call(name, "aic_rt_net_tcp_close", args, span, fctx))
            }
            "tcp_set_nodelay"
                if self.sig_matches_shape(name, &["Int", "Bool"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_set_socket_bool_option_call(
                    name,
                    "aic_rt_net_tcp_set_nodelay",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_get_nodelay"
                if self.sig_matches_shape(name, &["Int"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_get_socket_bool_option_call(
                    name,
                    "aic_rt_net_tcp_get_nodelay",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_set_keepalive"
                if self.sig_matches_shape(name, &["Int", "Bool"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_set_socket_bool_option_call(
                    name,
                    "aic_rt_net_tcp_set_keepalive",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_get_keepalive"
                if self.sig_matches_shape(name, &["Int"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_get_socket_bool_option_call(
                    name,
                    "aic_rt_net_tcp_get_keepalive",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_set_keepalive_idle_secs"
                if self.sig_matches_shape(name, &["Int", "Int"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_set_socket_int_option_call(
                    name,
                    "aic_rt_net_tcp_set_keepalive_idle_secs",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_get_keepalive_idle_secs"
                if self.sig_matches_shape(name, &["Int"], "Result[Int, NetError]") =>
            {
                Some(self.gen_net_get_socket_int_option_call(
                    name,
                    "aic_rt_net_tcp_get_keepalive_idle_secs",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_set_keepalive_interval_secs"
                if self.sig_matches_shape(name, &["Int", "Int"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_set_socket_int_option_call(
                    name,
                    "aic_rt_net_tcp_set_keepalive_interval_secs",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_get_keepalive_interval_secs"
                if self.sig_matches_shape(name, &["Int"], "Result[Int, NetError]") =>
            {
                Some(self.gen_net_get_socket_int_option_call(
                    name,
                    "aic_rt_net_tcp_get_keepalive_interval_secs",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_set_keepalive_count"
                if self.sig_matches_shape(name, &["Int", "Int"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_set_socket_int_option_call(
                    name,
                    "aic_rt_net_tcp_set_keepalive_count",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_get_keepalive_count"
                if self.sig_matches_shape(name, &["Int"], "Result[Int, NetError]") =>
            {
                Some(self.gen_net_get_socket_int_option_call(
                    name,
                    "aic_rt_net_tcp_get_keepalive_count",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_shutdown" if self.sig_matches_shape(name, &["Int"], "Result[Bool, NetError]") => {
                Some(self.gen_net_close_call(name, "aic_rt_net_tcp_shutdown", args, span, fctx))
            }
            "tcp_shutdown_read"
                if self.sig_matches_shape(name, &["Int"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_close_call(
                    name,
                    "aic_rt_net_tcp_shutdown_read",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_shutdown_write"
                if self.sig_matches_shape(name, &["Int"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_close_call(
                    name,
                    "aic_rt_net_tcp_shutdown_write",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_set_send_buffer_size"
                if self.sig_matches_shape(name, &["Int", "Int"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_set_socket_int_option_call(
                    name,
                    "aic_rt_net_tcp_set_send_buffer_size",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_get_send_buffer_size"
                if self.sig_matches_shape(name, &["Int"], "Result[Int, NetError]") =>
            {
                Some(self.gen_net_get_socket_int_option_call(
                    name,
                    "aic_rt_net_tcp_get_send_buffer_size",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_set_recv_buffer_size"
                if self.sig_matches_shape(name, &["Int", "Int"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_set_socket_int_option_call(
                    name,
                    "aic_rt_net_tcp_set_recv_buffer_size",
                    args,
                    span,
                    fctx,
                ))
            }
            "tcp_get_recv_buffer_size"
                if self.sig_matches_shape(name, &["Int"], "Result[Int, NetError]") =>
            {
                Some(self.gen_net_get_socket_int_option_call(
                    name,
                    "aic_rt_net_tcp_get_recv_buffer_size",
                    args,
                    span,
                    fctx,
                ))
            }
            "udp_close" if self.sig_matches_shape(name, &["Int"], "Result[Bool, NetError]") => {
                Some(self.gen_net_close_call(name, "aic_rt_net_udp_close", args, span, fctx))
            }
            "udp_send_to"
                if self.sig_matches_shape(
                    name,
                    &["Int", "String", "Bytes"],
                    "Result[Int, NetError]",
                ) || self.sig_matches_shape(
                    name,
                    &["Int", "String", "String"],
                    "Result[Int, NetError]",
                ) =>
            {
                Some(self.gen_net_udp_send_to_call(name, args, span, fctx))
            }
            "udp_recv_from"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[UdpPacket, NetError]",
                ) =>
            {
                Some(self.gen_net_udp_recv_from_call(name, args, span, fctx))
            }
            "dns_lookup"
                if self.sig_matches_shape(name, &["String"], "Result[String, NetError]") =>
            {
                Some(self.gen_net_dns_call(name, "aic_rt_net_dns_lookup", args, span, fctx))
            }
            "dns_lookup_all"
                if self.sig_matches_shape(name, &["String"], "Result[Vec[String], NetError]") =>
            {
                Some(self.gen_net_dns_lookup_all_call(name, args, span, fctx))
            }
            "dns_reverse"
                if self.sig_matches_shape(name, &["String"], "Result[String, NetError]") =>
            {
                Some(self.gen_net_dns_call(name, "aic_rt_net_dns_reverse", args, span, fctx))
            }
            "async_accept_submit"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int"],
                    "Result[AsyncIntOp, NetError]",
                ) =>
            {
                Some(self.gen_net_async_accept_submit_call(name, args, span, fctx))
            }
            "async_tcp_send_submit"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Bytes"],
                    "Result[AsyncIntOp, NetError]",
                ) || self.sig_matches_shape(
                    name,
                    &["Int", "String"],
                    "Result[AsyncIntOp, NetError]",
                ) =>
            {
                Some(self.gen_net_async_send_submit_call(name, args, span, fctx))
            }
            "async_tcp_recv_submit"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[AsyncStringOp, NetError]",
                ) =>
            {
                Some(self.gen_net_async_recv_submit_call(name, args, span, fctx))
            }
            "async_wait_int"
                if self.sig_matches_shape(
                    name,
                    &["AsyncIntOp", "Int"],
                    "Result[Int, NetError]",
                ) =>
            {
                Some(self.gen_net_async_wait_int_call(name, args, span, fctx))
            }
            "async_wait_string"
                if self.sig_matches_shape(
                    name,
                    &["AsyncStringOp", "Int"],
                    "Result[Bytes, NetError]",
                ) || self.sig_matches_shape(
                    name,
                    &["AsyncStringOp", "Int"],
                    "Result[String, NetError]",
                ) =>
            {
                Some(self.gen_net_async_wait_string_call(name, args, span, fctx))
            }
            "async_cancel_int"
                if self.sig_matches_shape(name, &["AsyncIntOp"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_async_cancel_call(
                    name,
                    args,
                    "AsyncIntOp",
                    "async_cancel_int",
                    "aic_rt_net_async_cancel",
                    span,
                    fctx,
                ))
            }
            "async_cancel_string"
                if self.sig_matches_shape(name, &["AsyncStringOp"], "Result[Bool, NetError]") =>
            {
                Some(self.gen_net_async_cancel_call(
                    name,
                    args,
                    "AsyncStringOp",
                    "async_cancel_string",
                    "aic_rt_net_async_cancel",
                    span,
                    fctx,
                ))
            }
            "async_shutdown" if self.sig_matches_shape(name, &[], "Result[Bool, NetError]") => {
                Some(self.gen_net_async_shutdown_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn gen_tls_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "aic_tls_connect_intrinsic" => "tls_connect",
            "aic_tls_connect_addr_intrinsic" => "tls_connect_addr",
            "aic_tls_accept_intrinsic" => "tls_accept",
            "aic_tls_send_intrinsic" => "tls_send",
            "aic_tls_send_timeout_intrinsic" => "tls_send_timeout",
            "aic_tls_recv_intrinsic" => "tls_recv",
            "aic_tls_async_send_submit_intrinsic" => "tls_async_send_submit",
            "aic_tls_async_recv_submit_intrinsic" => "tls_async_recv_submit",
            "aic_tls_async_wait_int_intrinsic" => "tls_async_wait_int",
            "aic_tls_async_wait_string_intrinsic" => "tls_async_wait_string",
            "aic_tls_async_cancel_int_intrinsic" => "tls_async_cancel_int",
            "aic_tls_async_cancel_string_intrinsic" => "tls_async_cancel_string",
            "aic_tls_async_shutdown_intrinsic" => "tls_async_shutdown",
            "aic_tls_close_intrinsic" => "tls_close",
            "aic_tls_peer_subject_intrinsic" => "tls_peer_subject",
            "aic_tls_peer_issuer_intrinsic" => "tls_peer_issuer",
            "aic_tls_peer_fingerprint_sha256_intrinsic" => "tls_peer_fingerprint_sha256",
            "aic_tls_peer_san_entries_intrinsic" => "tls_peer_san_entries",
            "aic_tls_version_intrinsic" => "tls_version",
            _ => return None,
        };

        match canonical {
            "tls_connect"
                if self.sig_matches_shape(
                    name,
                    &[
                        "Int", "Bool", "String", "Bool", "String", "Bool", "String", "Bool",
                        "String", "Bool",
                    ],
                    "Result[Int, TlsError]",
                ) =>
            {
                Some(self.gen_tls_connect_call(name, args, span, fctx))
            }
            "tls_connect_addr"
                if self.sig_matches_shape(
                    name,
                    &[
                        "String", "Bool", "String", "Bool", "String", "Bool", "String", "Bool",
                        "String", "Bool", "Int",
                    ],
                    "Result[Int, TlsError]",
                ) =>
            {
                Some(self.gen_tls_connect_addr_call(name, args, span, fctx))
            }
            "tls_accept"
                if self.sig_matches_shape(
                    name,
                    &[
                        "Int", "Bool", "String", "Bool", "String", "Bool", "String", "Bool", "Int",
                    ],
                    "Result[Int, TlsError]",
                ) =>
            {
                Some(self.gen_tls_accept_call(name, args, span, fctx))
            }
            "tls_send"
                if self.sig_matches_shape(name, &["Int", "String"], "Result[Int, TlsError]") =>
            {
                Some(self.gen_tls_send_call(name, args, span, fctx))
            }
            "tls_send_timeout"
                if self.sig_matches_shape(
                    name,
                    &["Int", "String", "Int"],
                    "Result[Int, TlsError]",
                ) =>
            {
                Some(self.gen_tls_send_timeout_call(name, args, span, fctx))
            }
            "tls_recv"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[String, TlsError]",
                ) =>
            {
                Some(self.gen_tls_recv_call(name, args, span, fctx))
            }
            "tls_async_send_submit"
                if self.sig_matches_shape(
                    name,
                    &["Int", "String", "Int"],
                    "Result[AsyncIntOp, TlsError]",
                ) =>
            {
                Some(self.gen_tls_async_send_submit_call(name, args, span, fctx))
            }
            "tls_async_recv_submit"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int", "Int"],
                    "Result[AsyncStringOp, TlsError]",
                ) =>
            {
                Some(self.gen_tls_async_recv_submit_call(name, args, span, fctx))
            }
            "tls_async_wait_int"
                if self.sig_matches_shape(
                    name,
                    &["AsyncIntOp", "Int"],
                    "Result[Int, TlsError]",
                ) =>
            {
                Some(self.gen_tls_async_wait_int_call(name, args, span, fctx))
            }
            "tls_async_wait_string"
                if self.sig_matches_shape(
                    name,
                    &["AsyncStringOp", "Int"],
                    "Result[String, TlsError]",
                ) || self.sig_matches_shape(
                    name,
                    &["AsyncStringOp", "Int"],
                    "Result[Bytes, TlsError]",
                ) =>
            {
                Some(self.gen_tls_async_wait_string_call(name, args, span, fctx))
            }
            "tls_async_cancel_int"
                if self.sig_matches_shape(name, &["AsyncIntOp"], "Result[Bool, TlsError]") =>
            {
                Some(self.gen_tls_async_cancel_call(
                    name,
                    args,
                    "AsyncIntOp",
                    "aic_tls_async_cancel_int_intrinsic",
                    span,
                    fctx,
                ))
            }
            "tls_async_cancel_string"
                if self.sig_matches_shape(name, &["AsyncStringOp"], "Result[Bool, TlsError]") =>
            {
                Some(self.gen_tls_async_cancel_call(
                    name,
                    args,
                    "AsyncStringOp",
                    "aic_tls_async_cancel_string_intrinsic",
                    span,
                    fctx,
                ))
            }
            "tls_async_shutdown" if self.sig_matches_shape(name, &[], "Result[Bool, TlsError]") => {
                Some(self.gen_tls_async_shutdown_call(name, args, span, fctx))
            }
            "tls_close" if self.sig_matches_shape(name, &["Int"], "Result[Bool, TlsError]") => {
                Some(self.gen_tls_close_call(name, args, span, fctx))
            }
            "tls_peer_subject"
                if self.sig_matches_shape(name, &["Int"], "Result[String, TlsError]") =>
            {
                Some(self.gen_tls_peer_subject_call(name, args, span, fctx))
            }
            "tls_peer_issuer"
                if self.sig_matches_shape(name, &["Int"], "Result[String, TlsError]") =>
            {
                Some(self.gen_tls_peer_issuer_call(name, args, span, fctx))
            }
            "tls_peer_fingerprint_sha256"
                if self.sig_matches_shape(name, &["Int"], "Result[String, TlsError]") =>
            {
                Some(self.gen_tls_peer_fingerprint_sha256_call(name, args, span, fctx))
            }
            "tls_peer_san_entries"
                if self.sig_matches_shape(name, &["Int"], "Result[Vec[String], TlsError]") =>
            {
                Some(self.gen_tls_peer_san_entries_call(name, args, span, fctx))
            }
            "tls_version" if self.sig_matches_shape(name, &["Int"], "Result[Int, TlsError]") => {
                Some(self.gen_tls_version_call(name, args, span, fctx))
            }
            _ => None,
        }
    }

    pub(super) fn bool_arg_to_i64(
        &mut self,
        value: &Value,
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<String> {
        if value.ty != LType::Bool {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects Bool argument"),
                self.file,
                span,
            ));
            return None;
        }
        let bool_i64 = self.new_temp();
        fctx.lines.push(format!(
            "  {} = zext i1 {} to i64",
            bool_i64,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        Some(bool_i64)
    }

    pub(super) fn gen_tls_connect_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 10 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_connect_intrinsic expects ten arguments",
                self.file,
                span,
            ));
            return None;
        }
        let tcp_fd = self.gen_expr(&args[0], fctx)?;
        let verify_server = self.gen_expr(&args[1], fctx)?;
        let ca_cert_path = self.gen_expr(&args[2], fctx)?;
        let has_ca_cert_path = self.gen_expr(&args[3], fctx)?;
        let client_cert_path = self.gen_expr(&args[4], fctx)?;
        let has_client_cert_path = self.gen_expr(&args[5], fctx)?;
        let client_key_path = self.gen_expr(&args[6], fctx)?;
        let has_client_key_path = self.gen_expr(&args[7], fctx)?;
        let server_name = self.gen_expr(&args[8], fctx)?;
        let has_server_name = self.gen_expr(&args[9], fctx)?;

        if tcp_fd.ty != LType::Int
            || ca_cert_path.ty != LType::String
            || client_cert_path.ty != LType::String
            || client_key_path.ty != LType::String
            || server_name.ty != LType::String
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_connect_intrinsic expects (Int, Bool, String, Bool, String, Bool, String, Bool, String, Bool)",
                self.file,
                span,
            ));
            return None;
        }

        let verify_server_i64 = self.bool_arg_to_i64(
            &verify_server,
            "aic_tls_connect_intrinsic",
            args[1].span,
            fctx,
        )?;
        let has_ca_cert_i64 = self.bool_arg_to_i64(
            &has_ca_cert_path,
            "aic_tls_connect_intrinsic",
            args[3].span,
            fctx,
        )?;
        let has_client_cert_i64 = self.bool_arg_to_i64(
            &has_client_cert_path,
            "aic_tls_connect_intrinsic",
            args[5].span,
            fctx,
        )?;
        let has_client_key_i64 = self.bool_arg_to_i64(
            &has_client_key_path,
            "aic_tls_connect_intrinsic",
            args[7].span,
            fctx,
        )?;
        let has_server_name_i64 = self.bool_arg_to_i64(
            &has_server_name,
            "aic_tls_connect_intrinsic",
            args[9].span,
            fctx,
        )?;

        let (ca_ptr, ca_len, ca_cap) = self.string_parts(&ca_cert_path, args[2].span, fctx)?;
        let (client_cert_ptr, client_cert_len, client_cert_cap) =
            self.string_parts(&client_cert_path, args[4].span, fctx)?;
        let (client_key_ptr, client_key_len, client_key_cap) =
            self.string_parts(&client_key_path, args[6].span, fctx)?;
        let (server_name_ptr, server_name_len, server_name_cap) =
            self.string_parts(&server_name, args[8].span, fctx)?;

        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_connect(i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i64* {})",
            err,
            tcp_fd.repr.clone().unwrap_or_else(|| "0".to_string()),
            verify_server_i64,
            ca_ptr,
            ca_len,
            ca_cap,
            has_ca_cert_i64,
            client_cert_ptr,
            client_cert_len,
            client_cert_cap,
            has_client_cert_i64,
            client_key_ptr,
            client_key_len,
            client_key_cap,
            has_client_key_i64,
            server_name_ptr,
            server_name_len,
            server_name_cap,
            has_server_name_i64,
            handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(handle),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_connect_addr_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 11 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_connect_addr_intrinsic expects eleven arguments",
                self.file,
                span,
            ));
            return None;
        }
        let addr = self.gen_expr(&args[0], fctx)?;
        let verify_server = self.gen_expr(&args[1], fctx)?;
        let ca_cert_path = self.gen_expr(&args[2], fctx)?;
        let has_ca_cert_path = self.gen_expr(&args[3], fctx)?;
        let client_cert_path = self.gen_expr(&args[4], fctx)?;
        let has_client_cert_path = self.gen_expr(&args[5], fctx)?;
        let client_key_path = self.gen_expr(&args[6], fctx)?;
        let has_client_key_path = self.gen_expr(&args[7], fctx)?;
        let server_name = self.gen_expr(&args[8], fctx)?;
        let has_server_name = self.gen_expr(&args[9], fctx)?;
        let timeout_ms = self.gen_expr(&args[10], fctx)?;

        if addr.ty != LType::String
            || ca_cert_path.ty != LType::String
            || client_cert_path.ty != LType::String
            || client_key_path.ty != LType::String
            || server_name.ty != LType::String
            || timeout_ms.ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_connect_addr_intrinsic expects (String, Bool, String, Bool, String, Bool, String, Bool, String, Bool, Int)",
                self.file,
                span,
            ));
            return None;
        }

        let verify_server_i64 = self.bool_arg_to_i64(
            &verify_server,
            "aic_tls_connect_addr_intrinsic",
            args[1].span,
            fctx,
        )?;
        let has_ca_cert_i64 = self.bool_arg_to_i64(
            &has_ca_cert_path,
            "aic_tls_connect_addr_intrinsic",
            args[3].span,
            fctx,
        )?;
        let has_client_cert_i64 = self.bool_arg_to_i64(
            &has_client_cert_path,
            "aic_tls_connect_addr_intrinsic",
            args[5].span,
            fctx,
        )?;
        let has_client_key_i64 = self.bool_arg_to_i64(
            &has_client_key_path,
            "aic_tls_connect_addr_intrinsic",
            args[7].span,
            fctx,
        )?;
        let has_server_name_i64 = self.bool_arg_to_i64(
            &has_server_name,
            "aic_tls_connect_addr_intrinsic",
            args[9].span,
            fctx,
        )?;

        let (addr_ptr, addr_len, addr_cap) = self.string_parts(&addr, args[0].span, fctx)?;
        let (ca_ptr, ca_len, ca_cap) = self.string_parts(&ca_cert_path, args[2].span, fctx)?;
        let (client_cert_ptr, client_cert_len, client_cert_cap) =
            self.string_parts(&client_cert_path, args[4].span, fctx)?;
        let (client_key_ptr, client_key_len, client_key_cap) =
            self.string_parts(&client_key_path, args[6].span, fctx)?;
        let (server_name_ptr, server_name_len, server_name_cap) =
            self.string_parts(&server_name, args[8].span, fctx)?;

        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_connect_addr(i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i64 {}, i64* {})",
            err,
            addr_ptr,
            addr_len,
            addr_cap,
            verify_server_i64,
            ca_ptr,
            ca_len,
            ca_cap,
            has_ca_cert_i64,
            client_cert_ptr,
            client_cert_len,
            client_cert_cap,
            has_client_cert_i64,
            client_key_ptr,
            client_key_len,
            client_key_cap,
            has_client_key_i64,
            server_name_ptr,
            server_name_len,
            server_name_cap,
            has_server_name_i64,
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(handle),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_accept_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 9 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_accept_intrinsic expects nine arguments",
                self.file,
                span,
            ));
            return None;
        }
        let listener_handle = self.gen_expr(&args[0], fctx)?;
        let verify_server = self.gen_expr(&args[1], fctx)?;
        let ca_cert_path = self.gen_expr(&args[2], fctx)?;
        let has_ca_cert_path = self.gen_expr(&args[3], fctx)?;
        let client_cert_path = self.gen_expr(&args[4], fctx)?;
        let has_client_cert_path = self.gen_expr(&args[5], fctx)?;
        let client_key_path = self.gen_expr(&args[6], fctx)?;
        let has_client_key_path = self.gen_expr(&args[7], fctx)?;
        let timeout_ms = self.gen_expr(&args[8], fctx)?;

        if listener_handle.ty != LType::Int
            || ca_cert_path.ty != LType::String
            || client_cert_path.ty != LType::String
            || client_key_path.ty != LType::String
            || timeout_ms.ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_accept_intrinsic expects (Int, Bool, String, Bool, String, Bool, String, Bool, Int)",
                self.file,
                span,
            ));
            return None;
        }

        let verify_server_i64 = self.bool_arg_to_i64(
            &verify_server,
            "aic_tls_accept_intrinsic",
            args[1].span,
            fctx,
        )?;
        let has_ca_cert_i64 = self.bool_arg_to_i64(
            &has_ca_cert_path,
            "aic_tls_accept_intrinsic",
            args[3].span,
            fctx,
        )?;
        let has_client_cert_i64 = self.bool_arg_to_i64(
            &has_client_cert_path,
            "aic_tls_accept_intrinsic",
            args[5].span,
            fctx,
        )?;
        let has_client_key_i64 = self.bool_arg_to_i64(
            &has_client_key_path,
            "aic_tls_accept_intrinsic",
            args[7].span,
            fctx,
        )?;

        let (ca_ptr, ca_len, ca_cap) = self.string_parts(&ca_cert_path, args[2].span, fctx)?;
        let (client_cert_ptr, client_cert_len, client_cert_cap) =
            self.string_parts(&client_cert_path, args[4].span, fctx)?;
        let (client_key_ptr, client_key_len, client_key_cap) =
            self.string_parts(&client_key_path, args[6].span, fctx)?;

        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_accept(i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i64 {}, i64* {})",
            err,
            listener_handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            verify_server_i64,
            ca_ptr,
            ca_len,
            ca_cap,
            has_ca_cert_i64,
            client_cert_ptr,
            client_cert_len,
            client_cert_cap,
            has_client_cert_i64,
            client_key_ptr,
            client_key_len,
            client_key_cap,
            has_client_key_i64,
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(handle),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_send_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_send_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let payload = self.gen_expr(&args[1], fctx)?;
        if handle.ty != LType::Int || payload.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_send_intrinsic expects (Int, String)",
                self.file,
                span,
            ));
            return None;
        }
        let (payload_ptr, payload_len, payload_cap) =
            self.string_parts(&payload, args[1].span, fctx)?;
        let out_sent_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_sent_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_send(i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            payload_ptr,
            payload_len,
            payload_cap,
            out_sent_slot
        ));
        let out_sent = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_sent, out_sent_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_sent),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_send_timeout_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_send_timeout_intrinsic expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let payload = self.gen_expr(&args[1], fctx)?;
        let timeout_ms = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || payload.ty != LType::String || timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_send_timeout_intrinsic expects (Int, String, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (payload_ptr, payload_len, payload_cap) =
            self.string_parts(&payload, args[1].span, fctx)?;
        let out_sent_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_sent_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_send_timeout(i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            payload_ptr,
            payload_len,
            payload_cap,
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_sent_slot
        ));
        let out_sent = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_sent, out_sent_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_sent),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_recv_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_recv_intrinsic expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let max_bytes = self.gen_expr(&args[1], fctx)?;
        let timeout_ms = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || max_bytes.ty != LType::Int || timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_recv_intrinsic expects (Int, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_recv(i64 {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            max_bytes.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let ok_payload = self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)?;
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_async_send_submit_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_async_send_submit_intrinsic expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let payload = self.gen_expr(&args[1], fctx)?;
        let timeout_ms = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || payload.ty != LType::String || timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_async_send_submit_intrinsic expects (Int, String, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (payload_ptr, payload_len, payload_cap) =
            self.string_parts(&payload, args[1].span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_async_send_submit(i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            payload_ptr,
            payload_len,
            payload_cap,
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let ok_payload =
            self.build_net_async_handle_payload(&result_ty, "AsyncIntOp", &out, span, fctx)?;
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_async_recv_submit_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_async_recv_submit_intrinsic expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let max_bytes = self.gen_expr(&args[1], fctx)?;
        let timeout_ms = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || max_bytes.ty != LType::Int || timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_async_recv_submit_intrinsic expects (Int, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_async_recv_submit(i64 {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            max_bytes.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let ok_payload =
            self.build_net_async_handle_payload(&result_ty, "AsyncStringOp", &out, span, fctx)?;
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_async_wait_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_async_wait_int_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let op = self.gen_expr(&args[0], fctx)?;
        let timeout = self.gen_expr(&args[1], fctx)?;
        if timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_async_wait_int_intrinsic expects (AsyncIntOp, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let op_handle = self.extract_named_handle_from_value(
            &op,
            "AsyncIntOp",
            "aic_tls_async_wait_int_intrinsic",
            args[0].span,
            fctx,
        )?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_async_wait_int(i64 {}, i64 {}, i64* {})",
            err,
            op_handle,
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_async_wait_string_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_async_wait_string_intrinsic expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let op = self.gen_expr(&args[0], fctx)?;
        let timeout = self.gen_expr(&args[1], fctx)?;
        if timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_async_wait_string_intrinsic expects (AsyncStringOp, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let op_handle = self.extract_named_handle_from_value(
            &op,
            "AsyncStringOp",
            "aic_tls_async_wait_string_intrinsic",
            args[0].span,
            fctx,
        )?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_async_wait_string(i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            op_handle,
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let data_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let ok_payload = if ok_ty == LType::String {
            data_value
        } else {
            self.build_bytes_value_from_data(
                &ok_ty,
                data_value,
                "aic_tls_async_wait_string_intrinsic",
                span,
                fctx,
            )?
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_async_cancel_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        op_ty_name: &str,
        context_name: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{context_name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let op = self.gen_expr(&args[0], fctx)?;
        let op_handle = self.extract_named_handle_from_value(
            &op,
            op_ty_name,
            context_name,
            args[0].span,
            fctx,
        )?;

        let cancelled_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", cancelled_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_async_cancel(i64 {}, i64* {})",
            err, op_handle, cancelled_slot
        ));
        let cancelled_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            cancelled_raw, cancelled_slot
        ));
        let cancelled = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp ne i64 {}, 0",
            cancelled, cancelled_raw
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some(cancelled),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_async_shutdown_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_async_shutdown_intrinsic expects no arguments",
                self.file,
                span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines
            .push(format!("  {} = call i64 @aic_rt_tls_async_shutdown()", err));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_close_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_close_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_close_intrinsic expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_close(i64 {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_peer_string_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        runtime_symbol: &str,
        intrinsic_name: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{intrinsic_name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{intrinsic_name} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i8** {}, i64* {})",
            err,
            runtime_symbol,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let ok_payload = self.load_string_from_out_slots(&out_ptr_slot, &out_len_slot, fctx)?;
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_peer_vec_string_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        runtime_symbol: &str,
        intrinsic_name: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{intrinsic_name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{intrinsic_name} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_items_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_items_slot));
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i8** {}, i64* {})",
            err,
            runtime_symbol,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_items_slot,
            out_count_slot
        ));
        let out_items = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_items, out_items_slot
        ));
        let out_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_count, out_count_slot
        ));
        let ok_payload =
            self.build_vec_string_payload_from_ptr(&out_items, &out_count, span, fctx)?;
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_tls_peer_subject_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.gen_tls_peer_string_call(
            name,
            args,
            "aic_rt_tls_peer_subject",
            "aic_tls_peer_subject_intrinsic",
            span,
            fctx,
        )
    }

    pub(super) fn gen_tls_peer_issuer_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.gen_tls_peer_string_call(
            name,
            args,
            "aic_rt_tls_peer_issuer",
            "aic_tls_peer_issuer_intrinsic",
            span,
            fctx,
        )
    }

    pub(super) fn gen_tls_peer_fingerprint_sha256_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.gen_tls_peer_string_call(
            name,
            args,
            "aic_rt_tls_peer_fingerprint_sha256",
            "aic_tls_peer_fingerprint_sha256_intrinsic",
            span,
            fctx,
        )
    }

    pub(super) fn gen_tls_peer_san_entries_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        self.gen_tls_peer_vec_string_call(
            name,
            args,
            "aic_rt_tls_peer_san_entries",
            "aic_tls_peer_san_entries_intrinsic",
            span,
            fctx,
        )
    }

    pub(super) fn gen_tls_version_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "aic_tls_version_intrinsic expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "aic_tls_version_intrinsic expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_version_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_version_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_tls_version(i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_version_slot
        ));
        let out_version = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_version, out_version_slot
        ));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_version),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_tls_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn wrap_tls_result(
        &mut self,
        result_ty: &LType,
        ok_payload: Value,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(result_ty, span)
        else {
            return None;
        };
        if ok_payload.ty != ok_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "tls builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }
        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_tls_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("tls_ok");
        let err_label = self.new_label("tls_err");
        let cont_label = self.new_label("tls_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            ok_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", err_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            reg,
            llvm_type(result_ty),
            llvm_type(result_ty),
            slot
        ));
        Some(Value {
            ty: result_ty.clone(),
            repr: Some(reg),
        })
    }

    pub(super) fn sig_matches_buffer_unit_result(&mut self, name: &str, params: &[&str]) -> bool {
        self.sig_matches_shape(name, params, "Result[(), BufferError]")
            || self.sig_matches_shape(name, params, "Result[Unit, BufferError]")
    }

    pub(super) fn gen_buffer_builtin_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Option<Value>> {
        let canonical = match name {
            "new_buffer" | "aic_buffer_new_intrinsic" => "new_buffer",
            "new_growable_buffer" | "aic_buffer_new_growable_intrinsic" => "new_growable_buffer",
            "buffer_from_bytes" | "aic_buffer_from_bytes_intrinsic" => "buffer_from_bytes",
            "buffer_to_bytes" | "aic_buffer_to_bytes_intrinsic" => "buffer_to_bytes",
            "buf_position" | "aic_buffer_position_intrinsic" => "buf_position",
            "buf_remaining" | "aic_buffer_remaining_intrinsic" => "buf_remaining",
            "buf_seek" | "aic_buffer_seek_intrinsic" => "buf_seek",
            "buf_reset" | "aic_buffer_reset_intrinsic" => "buf_reset",
            "buf_close" | "aic_buffer_close_intrinsic" => "buf_close",
            "buf_read_u8" | "aic_buffer_read_u8_intrinsic" => "buf_read_u8",
            "buf_read_i16_be" | "aic_buffer_read_i16_be_intrinsic" => "buf_read_i16_be",
            "buf_read_u16_be" | "aic_buffer_read_u16_be_intrinsic" => "buf_read_u16_be",
            "buf_read_i32_be" | "aic_buffer_read_i32_be_intrinsic" => "buf_read_i32_be",
            "buf_read_u32_be" | "aic_buffer_read_u32_be_intrinsic" => "buf_read_u32_be",
            "buf_read_i64_be" | "aic_buffer_read_i64_be_intrinsic" => "buf_read_i64_be",
            "buf_read_u64_be" | "aic_buffer_read_u64_be_intrinsic" => "buf_read_u64_be",
            "buf_read_i16_le" | "aic_buffer_read_i16_le_intrinsic" => "buf_read_i16_le",
            "buf_read_u16_le" | "aic_buffer_read_u16_le_intrinsic" => "buf_read_u16_le",
            "buf_read_i32_le" | "aic_buffer_read_i32_le_intrinsic" => "buf_read_i32_le",
            "buf_read_u32_le" | "aic_buffer_read_u32_le_intrinsic" => "buf_read_u32_le",
            "buf_read_i64_le" | "aic_buffer_read_i64_le_intrinsic" => "buf_read_i64_le",
            "buf_read_u64_le" | "aic_buffer_read_u64_le_intrinsic" => "buf_read_u64_le",
            "buf_read_bytes" | "aic_buffer_read_bytes_intrinsic" => "buf_read_bytes",
            "buf_read_cstring" | "aic_buffer_read_cstring_intrinsic" => "buf_read_cstring",
            "buf_read_length_prefixed" | "aic_buffer_read_length_prefixed_intrinsic" => {
                "buf_read_length_prefixed"
            }
            "buf_write_u8" | "aic_buffer_write_u8_intrinsic" => "buf_write_u8",
            "buf_write_i16_be" | "aic_buffer_write_i16_be_intrinsic" => "buf_write_i16_be",
            "buf_write_u16_be" | "aic_buffer_write_u16_be_intrinsic" => "buf_write_u16_be",
            "buf_write_i32_be" | "aic_buffer_write_i32_be_intrinsic" => "buf_write_i32_be",
            "buf_write_u32_be" | "aic_buffer_write_u32_be_intrinsic" => "buf_write_u32_be",
            "buf_write_i64_be" | "aic_buffer_write_i64_be_intrinsic" => "buf_write_i64_be",
            "buf_write_u64_be" | "aic_buffer_write_u64_be_intrinsic" => "buf_write_u64_be",
            "buf_write_i16_le" | "aic_buffer_write_i16_le_intrinsic" => "buf_write_i16_le",
            "buf_write_u16_le" | "aic_buffer_write_u16_le_intrinsic" => "buf_write_u16_le",
            "buf_write_i32_le" | "aic_buffer_write_i32_le_intrinsic" => "buf_write_i32_le",
            "buf_write_u32_le" | "aic_buffer_write_u32_le_intrinsic" => "buf_write_u32_le",
            "buf_write_i64_le" | "aic_buffer_write_i64_le_intrinsic" => "buf_write_i64_le",
            "buf_write_u64_le" | "aic_buffer_write_u64_le_intrinsic" => "buf_write_u64_le",
            "buf_write_bytes" | "aic_buffer_write_bytes_intrinsic" => "buf_write_bytes",
            "buf_write_cstring" | "aic_buffer_write_cstring_intrinsic" => "buf_write_cstring",
            "buf_write_string_prefixed" | "aic_buffer_write_string_prefixed_intrinsic" => {
                "buf_write_string_prefixed"
            }
            "buf_patch_u16_be" | "aic_buffer_patch_u16_be_intrinsic" => "buf_patch_u16_be",
            "buf_patch_u32_be" | "aic_buffer_patch_u32_be_intrinsic" => "buf_patch_u32_be",
            "buf_patch_u64_be" | "aic_buffer_patch_u64_be_intrinsic" => "buf_patch_u64_be",
            "buf_patch_u16_le" | "aic_buffer_patch_u16_le_intrinsic" => "buf_patch_u16_le",
            "buf_patch_u32_le" | "aic_buffer_patch_u32_le_intrinsic" => "buf_patch_u32_le",
            "buf_patch_u64_le" | "aic_buffer_patch_u64_le_intrinsic" => "buf_patch_u64_le",
            _ => return None,
        };

        match canonical {
            "new_buffer" if self.sig_matches_shape(name, &["Int"], "ByteBuffer") => {
                Some(self.gen_buffer_new_call(name, args, span, fctx))
            }
            "new_growable_buffer"
                if self.sig_matches_shape(
                    name,
                    &["Int", "Int"],
                    "Result[ByteBuffer, BufferError]",
                ) =>
            {
                Some(self.gen_buffer_new_growable_call(name, args, span, fctx))
            }
            "buffer_from_bytes" if self.sig_matches_shape(name, &["Bytes"], "ByteBuffer") => {
                Some(self.gen_buffer_from_bytes_call(name, args, span, fctx))
            }
            "buffer_to_bytes" if self.sig_matches_shape(name, &["ByteBuffer"], "Bytes") => {
                Some(self.gen_buffer_to_bytes_call(name, args, span, fctx))
            }
            "buf_position" if self.sig_matches_shape(name, &["ByteBuffer"], "Int") => {
                Some(self.gen_buffer_position_like_call(
                    "buf_position",
                    "aic_rt_buffer_position",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_remaining" if self.sig_matches_shape(name, &["ByteBuffer"], "Int") => {
                Some(self.gen_buffer_position_like_call(
                    "buf_remaining",
                    "aic_rt_buffer_remaining",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_seek" if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) => {
                Some(self.gen_buffer_seek_call(name, args, span, fctx))
            }
            "buf_reset"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Unit")
                    || self.sig_matches_shape(name, &["ByteBuffer"], "()") =>
            {
                Some(self.gen_buffer_reset_call(name, args, span, fctx))
            }
            "buf_close"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Bool, BufferError]") =>
            {
                Some(self.gen_buffer_close_call(name, args, span, fctx))
            }
            "buf_read_u8"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(name, "aic_rt_buffer_read_u8", args, span, fctx))
            }
            "buf_read_i16_be"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_i16_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_u16_be"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_u16_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_i32_be"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_i32_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_u32_be"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_u32_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_i64_be"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_i64_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_u64_be"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_u64_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_i16_le"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_i16_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_u16_le"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_u16_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_i32_le"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_i32_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_u32_le"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_u32_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_i64_le"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_i64_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_u64_le"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Int, BufferError]") =>
            {
                Some(self.gen_buffer_read_int_call(
                    name,
                    "aic_rt_buffer_read_u64_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_bytes"
                if self.sig_matches_shape(
                    name,
                    &["ByteBuffer", "Int"],
                    "Result[Bytes, BufferError]",
                ) =>
            {
                Some(self.gen_buffer_read_bytes_call(name, args, span, fctx))
            }
            "buf_read_cstring"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[String, BufferError]") =>
            {
                Some(self.gen_buffer_read_string_or_bytes_call(
                    name,
                    "aic_rt_buffer_read_cstring",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_read_length_prefixed"
                if self.sig_matches_shape(name, &["ByteBuffer"], "Result[Bytes, BufferError]") =>
            {
                Some(self.gen_buffer_read_string_or_bytes_call(
                    name,
                    "aic_rt_buffer_read_length_prefixed",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_u8" if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) => {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_u8",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_i16_be"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_i16_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_u16_be"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_u16_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_i32_be"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_i32_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_u32_be"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_u32_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_i64_be"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_i64_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_u64_be"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_u64_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_i16_le"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_i16_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_u16_le"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_u16_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_i32_le"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_i32_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_u32_le"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_u32_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_i64_le"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_i64_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_u64_le"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int"]) =>
            {
                Some(self.gen_buffer_write_int_call(
                    name,
                    "aic_rt_buffer_write_u64_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_bytes"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Bytes"]) =>
            {
                Some(self.gen_buffer_write_bytes_call(name, args, span, fctx))
            }
            "buf_write_cstring"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "String"]) =>
            {
                Some(self.gen_buffer_write_string_payload_call(
                    name,
                    "aic_rt_buffer_write_cstring",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_write_string_prefixed"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "String"]) =>
            {
                Some(self.gen_buffer_write_string_payload_call(
                    name,
                    "aic_rt_buffer_write_string_prefixed",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_patch_u16_be"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int", "Int"]) =>
            {
                Some(self.gen_buffer_patch_int_call(
                    name,
                    "aic_rt_buffer_patch_u16_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_patch_u32_be"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int", "Int"]) =>
            {
                Some(self.gen_buffer_patch_int_call(
                    name,
                    "aic_rt_buffer_patch_u32_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_patch_u64_be"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int", "Int"]) =>
            {
                Some(self.gen_buffer_patch_int_call(
                    name,
                    "aic_rt_buffer_patch_u64_be",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_patch_u16_le"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int", "Int"]) =>
            {
                Some(self.gen_buffer_patch_int_call(
                    name,
                    "aic_rt_buffer_patch_u16_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_patch_u32_le"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int", "Int"]) =>
            {
                Some(self.gen_buffer_patch_int_call(
                    name,
                    "aic_rt_buffer_patch_u32_le",
                    args,
                    span,
                    fctx,
                ))
            }
            "buf_patch_u64_le"
                if self.sig_matches_buffer_unit_result(name, &["ByteBuffer", "Int", "Int"]) =>
            {
                Some(self.gen_buffer_patch_int_call(
                    name,
                    "aic_rt_buffer_patch_u64_le",
                    args,
                    span,
                    fctx,
                ))
            }
            _ => None,
        }
    }

    pub(super) fn buffer_result_ty(
        &mut self,
        name: &str,
        span: crate::span::Span,
    ) -> Option<LType> {
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        Some(result_ty)
    }

    pub(super) fn build_buffer_value_from_handle(
        &mut self,
        buffer_ty: &LType,
        handle: &str,
        context: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let LType::Struct(layout) = buffer_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects ByteBuffer return type"),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != "ByteBuffer"
            || layout.fields.len() != 1
            || layout.fields[0].name != "handle"
            || layout.fields[0].ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{context} expects ByteBuffer return type"),
                self.file,
                span,
            ));
            return None;
        }
        self.build_struct_value(
            layout,
            &[Value {
                ty: LType::Int,
                repr: Some(handle.to_string()),
            }],
            span,
            fctx,
        )
    }

    pub(super) fn gen_buffer_new_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "new_buffer expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let capacity = self.gen_expr(&args[0], fctx)?;
        if capacity.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "new_buffer expects Int",
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_handle_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_handle_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", out_handle_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_buffer_new(i64 {}, i64* {})",
            _err,
            capacity.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, out_handle_slot));
        let result_ty = self
            .fn_sigs
            .get(name)
            .map(|sig| sig.ret.clone())
            .unwrap_or_else(|| {
                LType::Struct(StructLayoutType {
                    repr: "ByteBuffer".to_string(),
                    fields: vec![StructFieldType {
                        name: "handle".to_string(),
                        ty: LType::Int,
                    }],
                })
            });
        self.build_buffer_value_from_handle(&result_ty, &handle, "new_buffer", span, fctx)
    }

    pub(super) fn gen_buffer_new_growable_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "new_growable_buffer expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let initial_capacity = self.gen_expr(&args[0], fctx)?;
        let max_capacity = self.gen_expr(&args[1], fctx)?;
        if initial_capacity.ty != LType::Int || max_capacity.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "new_growable_buffer expects (Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_handle_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_handle_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", out_handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_buffer_new_growable(i64 {}, i64 {}, i64* {})",
            err,
            initial_capacity
                .repr
                .clone()
                .unwrap_or_else(|| "0".to_string()),
            max_capacity.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, out_handle_slot));
        let result_ty = self.buffer_result_ty(name, span)?;
        let buffer_ty = self.parse_type_repr("ByteBuffer", span)?;
        let ok_payload = self.build_buffer_value_from_handle(
            &buffer_ty,
            &handle,
            "new_growable_buffer",
            span,
            fctx,
        )?;
        self.wrap_buffer_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_buffer_from_bytes_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "buffer_from_bytes expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let bytes = self.gen_expr(&args[0], fctx)?;
        let (ptr, len, cap) = self.bytes_parts(&bytes, "buffer_from_bytes", args[0].span, fctx)?;
        let out_handle_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_handle_slot));
        fctx.lines
            .push(format!("  store i64 0, i64* {}", out_handle_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_buffer_from_bytes(i8* {}, i64 {}, i64 {}, i64* {})",
            _err, ptr, len, cap, out_handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, out_handle_slot));
        let result_ty = self
            .fn_sigs
            .get(name)
            .map(|sig| sig.ret.clone())
            .unwrap_or_else(|| {
                LType::Struct(StructLayoutType {
                    repr: "ByteBuffer".to_string(),
                    fields: vec![StructFieldType {
                        name: "handle".to_string(),
                        ty: LType::Int,
                    }],
                })
            });
        self.build_buffer_value_from_handle(&result_ty, &handle, "buffer_from_bytes", span, fctx)
    }

    pub(super) fn gen_buffer_to_bytes_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "buffer_to_bytes expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &buffer,
            "ByteBuffer",
            "buffer_to_bytes",
            args[0].span,
            fctx,
        )?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_buffer_to_bytes(i64 {}, i8** {}, i64* {})",
            _err, handle, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let data_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let result_ty = self
            .fn_sigs
            .get(name)
            .map(|sig| sig.ret.clone())
            .unwrap_or_else(|| self.parse_type_repr("Bytes", span).unwrap_or(LType::String));
        if result_ty == LType::String {
            return Some(data_value);
        }
        self.build_bytes_value_from_data(&result_ty, data_value, "buffer_to_bytes", span, fctx)
    }

    pub(super) fn gen_buffer_position_like_call(
        &mut self,
        context: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{context} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &buffer,
            "ByteBuffer",
            context,
            args[0].span,
            fctx,
        )?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        fctx.lines.push(format!("  store i64 0, i64* {}", out_slot));
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64* {})",
            _err, runtime_fn, handle, out_slot
        ));
        let out_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_value, out_slot));
        Some(Value {
            ty: LType::Int,
            repr: Some(out_value),
        })
    }

    pub(super) fn gen_buffer_seek_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "buf_seek expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let position = self.gen_expr(&args[1], fctx)?;
        if position.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "buf_seek expects Int position",
                self.file,
                args[1].span,
            ));
            return None;
        }
        let handle = self.extract_named_handle_from_value(
            &buffer,
            "ByteBuffer",
            "buf_seek",
            args[0].span,
            fctx,
        )?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_buffer_seek(i64 {}, i64 {})",
            err,
            handle,
            position.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let result_ty = self.buffer_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Unit,
            repr: None,
        };
        self.wrap_buffer_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_buffer_reset_call(
        &mut self,
        _name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "buf_reset expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &buffer,
            "ByteBuffer",
            "buf_reset",
            args[0].span,
            fctx,
        )?;
        let _err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_buffer_reset(i64 {})",
            _err, handle
        ));
        Some(Value {
            ty: LType::Unit,
            repr: None,
        })
    }

    pub(super) fn gen_buffer_close_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "buf_close expects one argument",
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &buffer,
            "ByteBuffer",
            "buf_close",
            args[0].span,
            fctx,
        )?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_buffer_close(i64 {})",
            err, handle
        ));
        let result_ty = self.buffer_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        self.wrap_buffer_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_buffer_read_int_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let handle =
            self.extract_named_handle_from_value(&buffer, "ByteBuffer", name, args[0].span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        fctx.lines.push(format!("  store i64 0, i64* {}", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64* {})",
            err, runtime_fn, handle, out_slot
        ));
        let out_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_value, out_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_value),
        };
        let result_ty = self.buffer_result_ty(name, span)?;
        self.wrap_buffer_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_buffer_read_bytes_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "buf_read_bytes expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let count = self.gen_expr(&args[1], fctx)?;
        if count.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "buf_read_bytes expects Int count",
                self.file,
                args[1].span,
            ));
            return None;
        }
        let handle = self.extract_named_handle_from_value(
            &buffer,
            "ByteBuffer",
            "buf_read_bytes",
            args[0].span,
            fctx,
        )?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_buffer_read_bytes(i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            handle,
            count.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let result_ty = self.buffer_result_ty(name, span)?;
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let data_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let ok_payload = if ok_ty == LType::String {
            data_value
        } else {
            self.build_bytes_value_from_data(&ok_ty, data_value, name, span, fctx)?
        };
        self.wrap_buffer_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_buffer_read_string_or_bytes_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let handle =
            self.extract_named_handle_from_value(&buffer, "ByteBuffer", name, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i8** {}, i64* {})",
            err, runtime_fn, handle, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let result_ty = self.buffer_result_ty(name, span)?;
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let data_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let ok_payload = if ok_ty == LType::String {
            data_value
        } else {
            self.build_bytes_value_from_data(&ok_ty, data_value, name, span, fctx)?
        };
        self.wrap_buffer_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_buffer_write_int_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects two arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let value = self.gen_expr(&args[1], fctx)?;
        if value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Int value"),
                self.file,
                args[1].span,
            ));
            return None;
        }
        let handle =
            self.extract_named_handle_from_value(&buffer, "ByteBuffer", name, args[0].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64 {})",
            err,
            runtime_fn,
            handle,
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let result_ty = self.buffer_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Unit,
            repr: None,
        };
        self.wrap_buffer_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_buffer_patch_int_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects three arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let offset = self.gen_expr(&args[1], fctx)?;
        let value = self.gen_expr(&args[2], fctx)?;
        if offset.ty != LType::Int || value.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects (ByteBuffer, Int, Int)"),
                self.file,
                span,
            ));
            return None;
        }
        let handle =
            self.extract_named_handle_from_value(&buffer, "ByteBuffer", name, args[0].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64 {}, i64 {})",
            err,
            runtime_fn,
            handle,
            offset.repr.clone().unwrap_or_else(|| "0".to_string()),
            value.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let result_ty = self.buffer_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Unit,
            repr: None,
        };
        self.wrap_buffer_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_buffer_write_bytes_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "buf_write_bytes expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let payload = self.gen_expr(&args[1], fctx)?;
        let handle = self.extract_named_handle_from_value(
            &buffer,
            "ByteBuffer",
            "buf_write_bytes",
            args[0].span,
            fctx,
        )?;
        let (ptr, len, cap) = self.bytes_parts(&payload, "buf_write_bytes", args[1].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_buffer_write_bytes(i64 {}, i8* {}, i64 {}, i64 {})",
            err, handle, ptr, len, cap
        ));
        let result_ty = self.buffer_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Unit,
            repr: None,
        };
        self.wrap_buffer_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_buffer_write_string_payload_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects two arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let buffer = self.gen_expr(&args[0], fctx)?;
        let payload = self.gen_expr(&args[1], fctx)?;
        if payload.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String payload"),
                self.file,
                args[1].span,
            ));
            return None;
        }
        let handle =
            self.extract_named_handle_from_value(&buffer, "ByteBuffer", name, args[0].span, fctx)?;
        let (ptr, len, cap) = self.string_parts(&payload, args[1].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i8* {}, i64 {}, i64 {})",
            err, runtime_fn, handle, ptr, len, cap
        ));
        let result_ty = self.buffer_result_ty(name, span)?;
        let ok_payload = Value {
            ty: LType::Unit,
            repr: None,
        };
        self.wrap_buffer_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_listen_or_bind_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let addr = self.gen_expr(&args[0], fctx)?;
        if addr.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&addr, args[0].span, fctx)?;
        let handle_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", handle_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i64* {})",
            err, runtime_fn, ptr, len, cap, handle_slot
        ));
        let handle = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", handle, handle_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(handle),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_local_addr_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i8** {}, i64* {})",
            err,
            runtime_fn,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let ok_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_tcp_accept_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "tcp_accept expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let listener = self.gen_expr(&args[0], fctx)?;
        let timeout = self.gen_expr(&args[1], fctx)?;
        if listener.ty != LType::Int || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "tcp_accept expects Int arguments",
                self.file,
                span,
            ));
            return None;
        }
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_accept(i64 {}, i64 {}, i64* {})",
            err,
            listener.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_tcp_connect_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "tcp_connect expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let addr = self.gen_expr(&args[0], fctx)?;
        let timeout = self.gen_expr(&args[1], fctx)?;
        if addr.ty != LType::String || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "tcp_connect expects (String, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&addr, args[0].span, fctx)?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_connect(i8* {}, i64 {}, i64 {}, i64 {}, i64* {})",
            err,
            ptr,
            len,
            cap,
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_tcp_send_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "tcp_send expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let payload = self.gen_expr(&args[1], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "tcp_send expects (Int, Bytes)",
                self.file,
                span,
            ));
            return None;
        }
        let (pptr, plen, pcap) = if payload.ty == LType::String {
            self.string_parts(&payload, args[1].span, fctx)?
        } else {
            self.bytes_parts(&payload, "tcp_send", args[1].span, fctx)?
        };
        let sent_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", sent_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_send(i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            pptr,
            plen,
            pcap,
            sent_slot
        ));
        let sent = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", sent, sent_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(sent),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_tcp_send_timeout_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "tcp_send_timeout expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let payload = self.gen_expr(&args[1], fctx)?;
        let timeout_ms = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || timeout_ms.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "tcp_send_timeout expects (Int, Bytes, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let (pptr, plen, pcap) = if payload.ty == LType::String {
            self.string_parts(&payload, args[1].span, fctx)?
        } else {
            self.bytes_parts(&payload, "tcp_send_timeout", args[1].span, fctx)?
        };
        let sent_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", sent_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_send_timeout(i64 {}, i8* {}, i64 {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            pptr,
            plen,
            pcap,
            timeout_ms.repr.clone().unwrap_or_else(|| "0".to_string()),
            sent_slot
        ));
        let sent = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", sent, sent_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(sent),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_tcp_recv_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "tcp_recv expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let max_bytes = self.gen_expr(&args[1], fctx)?;
        let timeout = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || max_bytes.ty != LType::Int || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "tcp_recv expects (Int, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_tcp_recv(i64 {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            max_bytes.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let ok_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_close_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {})",
            err,
            runtime_fn,
            handle.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_set_socket_bool_option_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects two arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects (Int, Bool)"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let enabled = self.gen_expr(&args[1], fctx)?;
        let enabled_i64 = self.bool_arg_to_i64(&enabled, name, args[1].span, fctx)?;
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64 {})",
            err,
            runtime_fn,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            enabled_i64
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_get_socket_bool_option_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64* {})",
            err,
            runtime_fn,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out_raw = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_raw, out_slot));
        let out_bool = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp ne i64 {}, 0", out_bool, out_raw));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some(out_bool),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_set_socket_int_option_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects two arguments"),
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let size_bytes = self.gen_expr(&args[1], fctx)?;
        if handle.ty != LType::Int || size_bytes.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects (Int, Int)"),
                self.file,
                span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64 {})",
            err,
            runtime_fn,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            size_bytes.repr.clone().unwrap_or_else(|| "0".to_string())
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_get_socket_int_option_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects Int"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64* {})",
            err,
            runtime_fn,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out_value = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_value, out_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out_value),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_udp_send_to_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "udp_send_to expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let addr = self.gen_expr(&args[1], fctx)?;
        let payload = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || addr.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "udp_send_to expects (Int, String, Bytes)",
                self.file,
                span,
            ));
            return None;
        }
        let (aptr, alen, acap) = self.string_parts(&addr, args[1].span, fctx)?;
        let (pptr, plen, pcap) = if payload.ty == LType::String {
            self.string_parts(&payload, args[2].span, fctx)?
        } else {
            self.bytes_parts(&payload, "udp_send_to", args[2].span, fctx)?
        };
        let sent_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", sent_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_udp_send_to(i64 {}, i8* {}, i64 {}, i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            aptr,
            alen,
            acap,
            pptr,
            plen,
            pcap,
            sent_slot
        ));
        let sent = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", sent, sent_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(sent),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_udp_recv_from_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "udp_recv_from expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let max_bytes = self.gen_expr(&args[1], fctx)?;
        let timeout = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || max_bytes.ty != LType::Int || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "udp_recv_from expects (Int, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let from_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", from_ptr_slot));
        let from_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", from_len_slot));
        let payload_ptr_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", payload_ptr_slot));
        let payload_len_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", payload_len_slot));

        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_udp_recv_from(i64 {}, i64 {}, i64 {}, i8** {}, i64* {}, i8** {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            max_bytes.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            from_ptr_slot,
            from_len_slot,
            payload_ptr_slot,
            payload_len_slot
        ));

        let from_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", from_ptr, from_ptr_slot));
        let from_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", from_len, from_len_slot));
        let payload_ptr = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            payload_ptr, payload_ptr_slot
        ));
        let payload_len = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            payload_len, payload_len_slot
        ));

        let from_value = self.build_string_value(&from_ptr, &from_len, &from_len, fctx);
        let payload_data = self.build_string_value(&payload_ptr, &payload_len, &payload_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let LType::Struct(ok_layout) = ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "udp_recv_from expects Result[UdpPacket, NetError] return type",
                self.file,
                span,
            ));
            return None;
        };
        let mut fields = Vec::with_capacity(ok_layout.fields.len());
        for field in &ok_layout.fields {
            match field.name.as_str() {
                "from" => {
                    if field.ty != LType::String {
                        self.diagnostics.push(Diagnostic::error(
                            "E5011",
                            "udp_recv_from expects UdpPacket.from to be String",
                            self.file,
                            span,
                        ));
                        return None;
                    }
                    fields.push(from_value.clone());
                }
                "payload" => {
                    let payload_value = if field.ty == LType::String {
                        payload_data.clone()
                    } else {
                        self.build_bytes_value_from_data(
                            &field.ty,
                            payload_data.clone(),
                            "udp_recv_from",
                            span,
                            fctx,
                        )?
                    };
                    fields.push(payload_value);
                }
                other => {
                    self.diagnostics.push(Diagnostic::error(
                        "E5011",
                        format!("udp_recv_from does not support UdpPacket field '{other}'"),
                        self.file,
                        span,
                    ));
                    return None;
                }
            }
        }
        let ok_payload = self.build_struct_value(&ok_layout, &fields, span, fctx)?;
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_dns_call(
        &mut self,
        name: &str,
        runtime_fn: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let input = self.gen_expr(&args[0], fctx)?;
        if input.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&input, args[0].span, fctx)?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err, runtime_fn, ptr, len, cap, out_ptr_slot, out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let ok_payload = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_dns_lookup_all_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let input = self.gen_expr(&args[0], fctx)?;
        if input.ty != LType::String {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("{name} expects String"),
                self.file,
                args[0].span,
            ));
            return None;
        }
        let (ptr, len, cap) = self.string_parts(&input, args[0].span, fctx)?;
        let out_items_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i8*", out_items_slot));
        let out_count_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", out_count_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_dns_lookup_all(i8* {}, i64 {}, i64 {}, i8** {}, i64* {})",
            err, ptr, len, cap, out_items_slot, out_count_slot
        ));
        let out_items = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i8*, i8** {}",
            out_items, out_items_slot
        ));
        let out_count = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            out_count, out_count_slot
        ));
        let ok_payload =
            self.build_vec_string_payload_from_ptr(&out_items, &out_count, span, fctx)?;
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn build_net_async_handle_payload(
        &mut self,
        result_ty: &LType,
        expected_name: &str,
        handle: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(result_ty, span) else {
            return None;
        };
        let LType::Struct(layout) = ok_ty else {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("net async builtin expects Result[{expected_name}, NetError] return type"),
                self.file,
                span,
            ));
            return None;
        };
        if base_type_name(&layout.repr) != expected_name
            || layout.fields.len() != 1
            || layout.fields[0].ty != LType::Int
        {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!("net async builtin expects Result[{expected_name}, NetError] return type"),
                self.file,
                span,
            ));
            return None;
        }
        self.build_struct_value(
            &layout,
            &[Value {
                ty: LType::Int,
                repr: Some(handle.to_string()),
            }],
            span,
            fctx,
        )
    }

    pub(super) fn gen_net_async_accept_submit_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "async_accept_submit expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let listener = self.gen_expr(&args[0], fctx)?;
        let timeout = self.gen_expr(&args[1], fctx)?;
        if listener.ty != LType::Int || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "async_accept_submit expects (Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_async_accept_submit(i64 {}, i64 {}, i64* {})",
            err,
            listener.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let ok_payload =
            self.build_net_async_handle_payload(&result_ty, "AsyncIntOp", &out, span, fctx)?;
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_async_send_submit_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "async_tcp_send_submit expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let payload = self.gen_expr(&args[1], fctx)?;
        if handle.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "async_tcp_send_submit expects (Int, Bytes)",
                self.file,
                span,
            ));
            return None;
        }
        let (ptr, len, cap) = if payload.ty == LType::String {
            self.string_parts(&payload, args[1].span, fctx)?
        } else {
            self.bytes_parts(&payload, "async_tcp_send_submit", args[1].span, fctx)?
        };
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_async_send_submit(i64 {}, i8* {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            ptr,
            len,
            cap,
            out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let ok_payload =
            self.build_net_async_handle_payload(&result_ty, "AsyncIntOp", &out, span, fctx)?;
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_async_recv_submit_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 3 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "async_tcp_recv_submit expects three arguments",
                self.file,
                span,
            ));
            return None;
        }
        let handle = self.gen_expr(&args[0], fctx)?;
        let max_bytes = self.gen_expr(&args[1], fctx)?;
        let timeout = self.gen_expr(&args[2], fctx)?;
        if handle.ty != LType::Int || max_bytes.ty != LType::Int || timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "async_tcp_recv_submit expects (Int, Int, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_async_recv_submit(i64 {}, i64 {}, i64 {}, i64* {})",
            err,
            handle.repr.clone().unwrap_or_else(|| "0".to_string()),
            max_bytes.repr.clone().unwrap_or_else(|| "0".to_string()),
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let ok_payload =
            self.build_net_async_handle_payload(&result_ty, "AsyncStringOp", &out, span, fctx)?;
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_async_wait_int_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "async_wait_int expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let op = self.gen_expr(&args[0], fctx)?;
        let timeout = self.gen_expr(&args[1], fctx)?;
        if timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "async_wait_int expects (AsyncIntOp, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let op_handle = self.extract_named_handle_from_value(
            &op,
            "AsyncIntOp",
            "async_wait_int",
            args[0].span,
            fctx,
        )?;
        let out_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_async_wait_int(i64 {}, i64 {}, i64* {})",
            err,
            op_handle,
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_slot
        ));
        let out = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out, out_slot));
        let ok_payload = Value {
            ty: LType::Int,
            repr: Some(out),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_async_wait_string_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 2 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "async_wait_string expects two arguments",
                self.file,
                span,
            ));
            return None;
        }
        let op = self.gen_expr(&args[0], fctx)?;
        let timeout = self.gen_expr(&args[1], fctx)?;
        if timeout.ty != LType::Int {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                "async_wait_string expects (AsyncStringOp, Int)",
                self.file,
                span,
            ));
            return None;
        }
        let op_handle = self.extract_named_handle_from_value(
            &op,
            "AsyncStringOp",
            "async_wait_string",
            args[0].span,
            fctx,
        )?;
        let out_ptr_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i8*", out_ptr_slot));
        let out_len_slot = self.new_temp();
        fctx.lines.push(format!("  {} = alloca i64", out_len_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @aic_rt_net_async_wait_string(i64 {}, i64 {}, i8** {}, i64* {})",
            err,
            op_handle,
            timeout.repr.clone().unwrap_or_else(|| "0".to_string()),
            out_ptr_slot,
            out_len_slot
        ));
        let out_ptr = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i8*, i8** {}", out_ptr, out_ptr_slot));
        let out_len = self.new_temp();
        fctx.lines
            .push(format!("  {} = load i64, i64* {}", out_len, out_len_slot));
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        let Some((_, ok_ty, _, _, _)) = self.result_layout_parts(&result_ty, span) else {
            return None;
        };
        let data_value = self.build_string_value(&out_ptr, &out_len, &out_len, fctx);
        let ok_payload = if ok_ty == LType::String {
            data_value
        } else {
            self.build_bytes_value_from_data(&ok_ty, data_value, "async_wait_string", span, fctx)?
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_async_cancel_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        op_ty_name: &str,
        context_name: &str,
        runtime_symbol: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if args.len() != 1 {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                format!("{context_name} expects one argument"),
                self.file,
                span,
            ));
            return None;
        }
        let op = self.gen_expr(&args[0], fctx)?;
        let op_handle = self.extract_named_handle_from_value(
            &op,
            op_ty_name,
            context_name,
            args[0].span,
            fctx,
        )?;

        let cancelled_slot = self.new_temp();
        fctx.lines
            .push(format!("  {} = alloca i64", cancelled_slot));
        let err = self.new_temp();
        fctx.lines.push(format!(
            "  {} = call i64 @{}(i64 {}, i64* {})",
            err, runtime_symbol, op_handle, cancelled_slot
        ));
        let cancelled_raw = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load i64, i64* {}",
            cancelled_raw, cancelled_slot
        ));
        let cancelled = self.new_temp();
        fctx.lines.push(format!(
            "  {} = icmp ne i64 {}, 0",
            cancelled, cancelled_raw
        ));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some(cancelled),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn gen_net_async_shutdown_call(
        &mut self,
        name: &str,
        args: &[ir::Expr],
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        if !args.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E5010",
                "async_shutdown expects no arguments",
                self.file,
                span,
            ));
            return None;
        }
        let err = self.new_temp();
        fctx.lines
            .push(format!("  {} = call i64 @aic_rt_net_async_shutdown()", err));
        let ok_payload = Value {
            ty: LType::Bool,
            repr: Some("1".to_string()),
        };
        let Some(result_ty) = self.fn_sigs.get(name).map(|sig| sig.ret.clone()) else {
            self.diagnostics.push(Diagnostic::error(
                "E5012",
                format!("unknown function '{name}' in codegen"),
                self.file,
                span,
            ));
            return None;
        };
        self.wrap_net_result(&result_ty, ok_payload, &err, span, fctx)
    }

    pub(super) fn wrap_net_result(
        &mut self,
        result_ty: &LType,
        ok_payload: Value,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(result_ty, span)
        else {
            return None;
        };
        if ok_payload.ty != ok_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "net builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_net_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("net_ok");
        let err_label = self.new_label("net_err");
        let cont_label = self.new_label("net_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            ok_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", err_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            reg,
            llvm_type(result_ty),
            llvm_type(result_ty),
            slot
        ));
        Some(Value {
            ty: result_ty.clone(),
            repr: Some(reg),
        })
    }

    pub(super) fn wrap_buffer_result(
        &mut self,
        result_ty: &LType,
        ok_payload: Value,
        err_code: &str,
        span: crate::span::Span,
        fctx: &mut FnCtx,
    ) -> Option<Value> {
        let Some((layout, ok_ty, err_ty, ok_index, err_index)) =
            self.result_layout_parts(result_ty, span)
        else {
            return None;
        };
        if ok_payload.ty != ok_ty {
            self.diagnostics.push(Diagnostic::error(
                "E5011",
                format!(
                    "buffer builtin ok payload expects '{}', found '{}'",
                    render_type(&ok_ty),
                    render_type(&ok_payload.ty)
                ),
                self.file,
                span,
            ));
            return None;
        }

        let ok_value = self.build_enum_variant(&layout, ok_index, Some(ok_payload), span, fctx)?;
        let err_payload = self.build_buffer_error_from_code(&err_ty, err_code, span, fctx)?;
        let err_value =
            self.build_enum_variant(&layout, err_index, Some(err_payload), span, fctx)?;

        let slot = self.alloc_entry_slot(result_ty, fctx);
        let is_ok = self.new_temp();
        fctx.lines
            .push(format!("  {} = icmp eq i64 {}, 0", is_ok, err_code));
        let ok_label = self.new_label("buffer_ok");
        let err_label = self.new_label("buffer_err");
        let cont_label = self.new_label("buffer_cont");
        fctx.lines.push(format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok, ok_label, err_label
        ));

        fctx.lines.push(format!("{}:", ok_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            ok_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", err_label));
        fctx.lines.push(format!(
            "  store {} {}, {}* {}",
            llvm_type(result_ty),
            err_value
                .repr
                .clone()
                .unwrap_or_else(|| default_value(result_ty)),
            llvm_type(result_ty),
            slot
        ));
        fctx.lines.push(format!("  br label %{}", cont_label));

        fctx.lines.push(format!("{}:", cont_label));
        let reg = self.new_temp();
        fctx.lines.push(format!(
            "  {} = load {}, {}* {}",
            reg,
            llvm_type(result_ty),
            llvm_type(result_ty),
            slot
        ));
        Some(Value {
            ty: result_ty.clone(),
            repr: Some(reg),
        })
    }
}
