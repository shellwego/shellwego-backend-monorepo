# ShellWeGo Network – Build Targets
#
# The primary non-Rust target is compiling the eBPF programs.
# The resulting object file is renamed to shellwego.bin and placed
# in src/ebpf/bin/ where it is embedded via include_bytes!().

# ---------------------------------------------------------------------------
# eBPF compilation
# ---------------------------------------------------------------------------

CLANG    ?= clang
LLC      ?= llc
BPFTOOL  ?= bpftool

# Output directory for compiled eBPF binary
EBPF_BIN_DIR := src/ebpf/bin
EBPF_SRC_DIR := src/ebpf/programs

# The final binary that gets include_bytes!()'d into the Rust crate
EBPF_BIN := $(EBPF_BIN_DIR)/shellwego.bin

# Individual object files
INGRESS_OBJ  := $(EBPF_SRC_DIR)/ingress_filter.bpf.o
EGRESS_OBJ   := $(EBPF_SRC_DIR)/tc_egress_limiter.bpf.o

# Common clang flags for BPF
BPF_CFLAGS := -target bpf \
              -g -O2 \
              -D__TARGET_ARCH_x86 \
              -I/usr/include

.PHONY: ebpf clean-ebpf help

# Default target
help:
	@echo "ShellWeGo Network - Build Targets"
	@echo ""
	@echo "  make ebpf        Compile eBPF programs -> $(EBPF_BIN)"
	@echo "  make clean-ebpf  Remove compiled eBPF artifacts"
	@echo ""
	@echo "Prerequisites:"
	@echo "  clang >= 14, llvm >= 14, libbpf-dev, kernel headers"
	@echo ""
	@echo "Ubuntu/Debian:"
	@echo "  sudo apt install clang llvm libbpf-dev linux-headers-$$(uname -r)"

# Compile both eBPF programs and combine into shellwego.bin
ebpf: $(INGRESS_OBJ) $(EGRESS_OBJ) | $(EBPF_BIN_DIR)
	@echo "[eBPF] Combining objects into $(EBPF_BIN)"
	@# Concatenate the two ELF objects.  Aya can load multiple programs
	@# from a single ELF file if they have distinct SEC() names.
	cat $(INGRESS_OBJ) $(EGRESS_OBJ) > $(EBPF_BIN)
	@echo "[eBPF] Done: $(EBPF_BIN)"

$(INGRESS_OBJ): $(EBPF_SRC_DIR)/ingress_filter.bpf.c
	@echo "[eBPF] Compiling $<"
	$(CLANG) $(BPF_CFLAGS) -c $< -o $@

$(EGRESS_OBJ): $(EBPF_SRC_DIR)/tc_egress_limiter.bpf.c
	@echo "[eBPF] Compiling $<"
	$(CLANG) $(BPF_CFLAGS) -c $< -o $@

$(EBPF_BIN_DIR):
	mkdir -p $(EBPF_BIN_DIR)

clean-ebpf:
	rm -f $(INGRESS_OBJ) $(EGRESS_OBJ) $(EBPF_BIN)
	@echo "[eBPF] Cleaned"

# ---------------------------------------------------------------------------
# Notes
# ---------------------------------------------------------------------------
#
# Why cat instead of bpftool gen?
#   Aya's Ebpf::load() expects a single ELF binary containing all programs.
#   For production, use `bpftool gen object` to properly merge objects:
#
#     bpftool gen object $(EBPF_BIN) $(INGRESS_OBJ) $(EGRESS_OBJ)
#
#   The `cat` approach works when programs don't share maps.  If you
#   encounter issues, install bpftool and switch to the gen command.
#
# BPF_MAP_TYPE_PERCPU_HASH availability:
#   Requires kernel >= 4.6.  The TC egress limiter uses per-CPU maps
#   to avoid lock contention on the token bucket state.
#
# PIN_BY_NAME maps:
#   The eBPF programs declare maps with LIBBPF_PIN_BY_NAME so that
#   user-space can look them up by name via the BPF filesystem at
#   /sys/fs/bpf/.  This requires bpffs to be mounted:
#
#     sudo mount bpffs /sys/fs/bpf -t bpf
