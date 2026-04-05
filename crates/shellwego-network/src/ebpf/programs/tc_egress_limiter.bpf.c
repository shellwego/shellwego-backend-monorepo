// SPDX-License-Identifier: GPL-2.0
// tc_egress_limiter.bpf.c - TC egress rate limiter for ShellWeGo
//
// Attaches to the TC egress hook to enforce per-interface bandwidth limits
// using a token-bucket algorithm.
//
// Return codes:
//   TC_ACT_OK   - Allow packet through
//   TC_ACT_SHOT - Drop packet
//
// Compile with:
//   clang -target bpf -g -O2 -c tc_egress_limiter.bpf.c -o tc_egress_limiter.o

#include <linux/bpf.h>
#include <linux/if_ether.h>
#include <linux/pkt_cls.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_endian.h>

char LICENSE[] SEC("license") = "GPL";

// ---------------------------------------------------------------------------
// BPF Maps
// ---------------------------------------------------------------------------

// Rate configuration per interface.
// key = interface index (u32), value = struct rate_config
struct rate_config {
    __u64 rate_bytes_per_sec;  // Allowed bytes per second
    __u64 burst_bytes;         // Maximum burst allowance in bytes
};

struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 256);
    __type(key, __u32);
    __type(value, struct rate_config);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} rate_config SEC(".maps");

// Per-CPU token bucket state per interface.
struct bucket_state {
    __u64 tokens;       // Current token count
    __u64 last_update;  // Timestamp of last update (nanoseconds)
};

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_HASH);
    __uint(max_entries, 256);
    __type(key, __u32);
    __type(value, struct bucket_state);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} egress_buckets SEC(".maps");

// Per-CPU statistics.
enum egress_stat_key {
    EGRESS_STAT_BYTES_SENT     = 0,
    EGRESS_STAT_BYTES_DROPPED  = 1,
    EGRESS_STAT_PKTS_SENT      = 2,
    EGRESS_STAT_PKTS_DROPPED   = 3,
    _EGRESS_STAT_MAX,
};

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, _EGRESS_STAT_MAX);
    __type(key, __u32);
    __type(value, __u64);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} egress_stats SEC(".maps");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

static __always_inline void egress_increment_stat(__u32 key, __u64 val)
{
    __u64 *cnt = bpf_map_lookup_elem(&egress_stats, &key);
    if (cnt)
        *cnt += val;
}

// Initialize or retrieve the per-CPU bucket for the given interface.
// Returns a pointer to the bucket, or NULL on allocation failure.
static __always_inline struct bucket_state *get_or_init_bucket(__u32 ifindex)
{
    struct bucket_state *bucket = bpf_map_lookup_elem(&egress_buckets, &ifindex);
    if (bucket)
        return bucket;

    // First time: create an empty bucket entry.
    struct bucket_state init = {};
    long err = bpf_map_update_elem(&egress_buckets, &ifindex, &init, BPF_NOEXIST);
    if (err != 0)
        return NULL;

    return bpf_map_lookup_elem(&egress_buckets, &ifindex);
}

// ---------------------------------------------------------------------------
// TC classifier program (egress)
// ---------------------------------------------------------------------------

SEC("classifier/egress_limiter")
int tc_egress_limiter(struct __sk_buff *skb)
{
    __u32 ifindex = skb->ifindex;
    __u32 pkt_len  = skb->len;

    // Look up the rate configuration for this interface.
    struct rate_config *cfg = bpf_map_lookup_elem(&rate_config, &ifindex);
    if (!cfg)
        return TC_ACT_OK; // No rate limit configured -> pass

    if (cfg->rate_bytes_per_sec == 0 && cfg->burst_bytes == 0)
        return TC_ACT_OK; // Disabled -> pass

    // Get or create token bucket
    struct bucket_state *bucket = get_or_init_bucket(ifindex);
    if (!bucket)
        return TC_ACT_OK; // Can't track -> allow through

    __u64 now = bpf_ktime_get_ns();

    // Refill tokens based on elapsed time
    __u64 elapsed = 0;
    if (now > bucket->last_update)
        elapsed = now - bucket->last_update;

    if (elapsed > 0) {
        // Refill: elapsed_ns * rate_bytes_per_sec / 1_000_000_000
        __u64 refill = (elapsed * cfg->rate_bytes_per_sec) / 1000000000ULL;
        if (refill > 0) {
            __u64 new_tokens = bucket->tokens + refill;
            if (new_tokens > cfg->burst_bytes)
                new_tokens = cfg->burst_bytes;
            bucket->tokens = new_tokens;
            bucket->last_update = now;
        }
    }

    // Check if we have enough tokens for this packet
    if (bucket->tokens < pkt_len) {
        // Not enough tokens -> drop
        egress_increment_stat(EGRESS_STAT_BYTES_DROPPED, pkt_len);
        egress_increment_stat(EGRESS_STAT_PKTS_DROPPED, 1);
        return TC_ACT_SHOT;
    }

    // Deduct tokens and allow
    bucket->tokens -= pkt_len;
    egress_increment_stat(EGRESS_STAT_BYTES_SENT, pkt_len);
    egress_increment_stat(EGRESS_STAT_PKTS_SENT, 1);

    return TC_ACT_OK;
}
