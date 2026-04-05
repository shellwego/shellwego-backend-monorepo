// SPDX-License-Identifier: GPL-2.0
// ingress_filter.bpf.c - XDP packet filter for ShellWeGo
//
// Attaches to the XDP hook on a network interface to provide:
//   - Per-source-IP packet counting
//   - IP blocklist (simple hash map)
//   - Per-IP rate limiting (token bucket)
//
// Return codes:
//   XDP_PASS  - Allow packet through
//   XDP_DROP  - Silently drop packet
//
// Compile with:
//   clang -target bpf -g -O2 -c ingress_filter.bpf.c -o ingress_filter.o

#include <linux/bpf.h>
#include <linux/if_ether.h>
#include <linux/if_packet.h>
#include <linux/ip.h>
#include <linux/ipv6.h>
#include <linux/in.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_endian.h>

char LICENSE[] SEC("license") = "GPL";

// ---------------------------------------------------------------------------
// BPF Maps
// ---------------------------------------------------------------------------

// blocked_ips: key = u32 (IPv4 address in network byte order), value = u8 (1=blocked)
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 65536);
    __type(key, __u32);
    __type(value, __u8);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} blocked_ips SEC(".maps");

// Rate limit state per source IP.
struct rate_limit_state {
    __u64 tokens;       // Current token count
    __u64 last_update;  // Timestamp of last update (nanoseconds)
    __u32 rate;         // Tokens per second
    __u32 burst;        // Maximum burst size
};

// rate_limits: key = u32 (IPv4), value = struct rate_limit_state
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 65536);
    __type(key, __u32);
    __type(value, struct rate_limit_state);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} rate_limits SEC(".maps");

// Per-CPU packet statistics indexed by action.
enum stat_key {
    STAT_PACKETS_TOTAL = 0,
    STAT_PACKETS_ALLOWED = 1,
    STAT_PACKETS_BLOCKED = 2,
    STAT_PACKETS_RATE_LIMITED = 3,
    STAT_BYTES_ALLOWED = 4,
    STAT_BYTES_BLOCKED = 5,
    _STAT_MAX,
};

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, _STAT_MAX);
    __type(key, __u32);
    __type(value, __u64);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} packet_stats SEC(".maps");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

static __always_inline void increment_stat(__u32 key, __u64 val)
{
    __u64 *cnt = bpf_map_lookup_elem(&packet_stats, &key);
    if (cnt)
        *cnt += val;
}

// Check if an IPv4 address is in the blocklist.
// Returns 1 if blocked, 0 if allowed or not found.
static __always_inline int is_blocked(__u32 src_ip)
{
    __u8 *blocked = bpf_map_lookup_elem(&blocked_ips, &src_ip);
    return (blocked && *blocked == 1) ? 1 : 0;
}

// Token-bucket rate limiter.
// Returns 1 if the packet should be dropped, 0 if allowed.
static __always_inline int check_rate_limit(__u32 src_ip, __u64 now)
{
    struct rate_limit_state *state = bpf_map_lookup_elem(&rate_limits, &src_ip);
    if (!state)
        return 0; // No rate limit configured for this IP

    __u64 elapsed = 0;
    if (now > state->last_update)
        elapsed = now - state->last_update;

    // Refill tokens: elapsed_ns * rate / 1_000_000_000
    __u64 refill = (elapsed * state->rate) / 1000000000ULL;
    if (refill > 0) {
        // Saturating add
        __u64 new_tokens = state->tokens + refill;
        if (new_tokens > state->burst)
            new_tokens = state->burst;
        state->tokens = new_tokens;
        state->last_update = now;
    }

    // Each packet costs 1 token (simplified; could use packet size).
    if (state->tokens == 0)
        return 1; // No tokens -> drop

    state->tokens -= 1;
    return 0;
}

// ---------------------------------------------------------------------------
// XDP program
// ---------------------------------------------------------------------------

SEC("xdp")
int ingress_filter(struct xdp_md *ctx)
{
    void *data_end = (void *)(long)ctx->data_end;
    void *data     = (void *)(long)ctx->data;

    // Parse Ethernet header
    struct ethhdr *eth = data;
    if ((void *)(eth + 1) > data_end)
        return XDP_PASS; // Cannot parse, let kernel handle

    // Only handle IPv4
    if (eth->h_proto != bpf_htons(ETH_P_IP))
        return XDP_PASS;

    struct iphdr *ip = (void *)(eth + 1);
    if ((void *)(ip + 1) > data_end)
        return XDP_PASS;

    __u32 src_ip = ip->saddr;
    __u32 pkt_len = (__u32)(data_end - data);

    // Update total packet counter
    increment_stat(STAT_PACKETS_TOTAL, 1);

    // 1. Check blocklist
    if (is_blocked(src_ip)) {
        increment_stat(STAT_PACKETS_BLOCKED, 1);
        increment_stat(STAT_BYTES_BLOCKED, pkt_len);
        return XDP_DROP;
    }

    // 2. Check rate limit (token bucket)
    __u64 now = bpf_ktime_get_ns();
    if (check_rate_limit(src_ip, now)) {
        increment_stat(STAT_PACKETS_RATE_LIMITED, 1);
        increment_stat(STAT_BYTES_BLOCKED, pkt_len);
        return XDP_DROP;
    }

    // 3. Allowed
    increment_stat(STAT_PACKETS_ALLOWED, 1);
    increment_stat(STAT_BYTES_ALLOWED, pkt_len);
    return XDP_PASS;
}
