# ==========================================
# ZIL FRAMEWORK MASTER MAKEFILE
# Target: Apple A19 Pro (arm64e) / iOS 26
# ==========================================

# --- 1. TOOLCHAIN DEFINITIONS ---
# Kita menggunakan Clang bawaan Xcode, tetapi dipaksa bertindak sebagai cross-compiler bare-metal
CC := $(shell xcrun -sdk iphoneos -find clang)
AS := $(shell xcrun -sdk iphoneos -find as)
LD := $(shell xcrun -sdk iphoneos -find ld)
CARGO := cargo

# --- 2. ARCHITECTURE & FLAGS ---
ARCH := arm64e
TARGET_TRIPLE := aarch64-apple-ios

# C Flags untuk Kernel Space (Sangat Kritis)
# -nostdlib & -ffreestanding: Dilarang menggunakan printf, malloc, atau libc
# -mno-implicit-floats: Mencegah compiler menggunakan register NEON/FPU secara diam-diam (bisa merusak state kernel)
CFLAGS := -arch $(ARCH) -isysroot $(shell xcrun -sdk iphoneos --show-sdk-path) \
          -O3 -ffreestanding -nostdlib -mno-implicit-floats \
          -Wall -Wextra -Iinclude -Iarch/arm64 -Idriver

# Assembly Flags
ASFLAGS := -arch $(ARCH)

# Linker Flags
# Menghubungkan biner menggunakan peta memori ZIL (200MB Logic Space)
LDFLAGS := -arch $(ARCH) -T linker.ld -e _start -static

# --- 3. DIRECTORIES ---
SRC_ARCH := arch/arm64
BUILD_DIR := build
OBJ_DIR := $(BUILD_DIR)/obj
BIN_DIR := $(BUILD_DIR)/bin

# --- 4. SOURCE FILES ---
# Arch-specific ASM/C (boot, PAC, safe-read, WFI)
ASM_SRCS := $(wildcard $(SRC_ARCH)/*.s)
ARCH_C_SRCS := $(wildcard $(SRC_ARCH)/*.c)

# Driver C files — iokit_shim.c, ane_asymmetric.c, agx_compute.c
DRIVER_C_SRCS := $(wildcard driver/npu/*.c) $(wildcard driver/gpu/*.c) $(wildcard driver/*.c)

# Merge semua C sources
C_SRCS := $(ARCH_C_SRCS) $(DRIVER_C_SRCS)

# Object files — gunakan basename agar tidak bentrok path separator
ASM_OBJS   := $(patsubst $(SRC_ARCH)/%.s, $(OBJ_DIR)/%.o, $(ASM_SRCS))
ARCH_C_OBJ := $(patsubst $(SRC_ARCH)/%.c, $(OBJ_DIR)/%.o, $(ARCH_C_SRCS))
DRVR_C_OBJ := $(patsubst %.c, $(OBJ_DIR)/drv_%.o, $(notdir $(DRIVER_C_SRCS)))
C_OBJS     := $(ARCH_C_OBJ) $(DRVR_C_OBJ)

# --- 5. RUST WORKSPACE TARGETS ---
# Lokasi file static library (.a) hasil kompilasi Cargo
RUST_PATHFINDER_LIB := core/target/$(TARGET_TRIPLE)/release/libzil_pathfinder.a
RUST_EXECUTOR_LIB := core/target/$(TARGET_TRIPLE)/release/libzil_executor.a

# --- 6. TARGETS (ATURAN BUILD) ---

.PHONY: all clean rust_core dirs

all: dirs pathfinder executor

# Membuat struktur direktori build
dirs:
	@mkdir -p $(OBJ_DIR)
	@mkdir -p $(BIN_DIR)

# Kompilasi file Assembly (.s -> .o)
$(OBJ_DIR)/%.o: $(SRC_ARCH)/%.s
	@echo "[+] Merakit Assembly: $<"
	@$(CC) $(ASFLAGS) -c $< -o $@

# Kompilasi file C dari arch/arm64 (.c -> .o)
$(OBJ_DIR)/%.o: $(SRC_ARCH)/%.c
	@echo "[+] Mengompilasi C (arch): $<"
	@$(CC) $(CFLAGS) -c $< -o $@

# Kompilasi file C dari driver/ (disimpan sebagai drv_*.o)
$(OBJ_DIR)/drv_%.o: driver/npu/%.c
	@echo "[+] Mengompilasi C (driver/npu): $<"
	@$(CC) $(CFLAGS) -c $< -o $@

$(OBJ_DIR)/drv_%.o: driver/gpu/%.c
	@echo "[+] Mengompilasi C (driver/gpu): $<"
	@$(CC) $(CFLAGS) -c $< -o $@

$(OBJ_DIR)/drv_%.o: driver/%.c
	@echo "[+] Mengompilasi C (driver): $<"
	@$(CC) $(CFLAGS) -c $< -o $@

# Memanggil Cargo untuk kompilasi modul Rust
rust_core:
	@echo "[+] Mengompilasi Otak Logika (Rust no_std)..."
	@cd core && $(CARGO) build --target $(TARGET_TRIPLE) --release

# --- 7. LINKING BINARIES (Tahap Akhir) ---

# Biner A: Pathfinder (Pengintai)
pathfinder: dirs $(ASM_OBJS) $(C_OBJS) rust_core
	@echo "[+] Menyatukan Biner A (Pathfinder)..."
	@$(LD) $(LDFLAGS) $(ASM_OBJS) $(C_OBJS) $(RUST_PATHFINDER_LIB) -o $(BIN_DIR)/pathfinder.bin
	@echo "    -> Selesai: $(BIN_DIR)/pathfinder.bin"

# Biner B: Executor (Mesin Utama + NPU)
executor: dirs $(ASM_OBJS) $(C_OBJS) rust_core
	@echo "[+] Menyatukan Biner B (Executor)..."
	@$(LD) $(LDFLAGS) $(ASM_OBJS) $(C_OBJS) $(RUST_EXECUTOR_LIB) -o $(BIN_DIR)/executor.bin
	@echo "    -> Selesai: $(BIN_DIR)/executor.bin"

# Bersihkan artefak
clean:
	@echo "[!] Membersihkan sisa build..."
	@rm -rf $(BUILD_DIR)
	@cd core && $(CARGO) clean