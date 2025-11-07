-- This was missed in the previous uint migration

alter table ips_with_aliased_servers alter column allowed_port set data type uint2 using ((allowed_port::bigint + 65536::bigint) % 65536::bigint)::uint2;
