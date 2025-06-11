CONFIG ?= debug
SHELL = /bin/bash
BLR_TARGET = x86_64-unknown-uefi
KERNEL_ARCH = X86_64
ENV_PLACEHOLDER = placeholder.txt
KERN_PLACEHOLDER = kernel/placeholder_test.txt
KERNEL_TARGET = config/$(KERNEL_ARCH)/$(KERNEL_ARCH).json
BLR_CRATE_PATH = boot/uefi
KERNEL_CRATE_PATH = kernel
BLR_EXE = target/$(BLR_TARGET)/$(CONFIG)/boot.efi
KERNEL_EXE = target/$(KERNEL_ARCH)/$(CONFIG)/libaris.so
LINKER_SCRIPT = $(KERNEL_CRATE_PATH)/config/$(KERNEL_ARCH)/linker.ld
OUTPUT_DIR = output
IMAGE_NAME = disk-tools
GEN_MSG = "Automatically generated file..\nDo not remove manually.."
DRIVER_DIRS := $(wildcard kernel/src/drivers/*)

ifeq ($(OS),Windows_NT)
    RUN_DOCKER_SCRIPT = @./scripts/docker.bat
else
    RUN_DOCKER_SCRIPT = @docker run -it --privileged -v "$$(pwd)":/workspace -w /workspace $(IMAGE_NAME) ./scripts/create_image_uefi.sh
endif

ifeq ($(CONFIG),release)
	BUILD_OPTIONS := --release
else ifeq ($(CONFIG),debug)
	BUILD_OPTIONS :=
else 
$(error Config flag must be either 'debug' or 'release')
endif


.PHONY: all clean build_blr build_kernel build_image

all: build_image

$(ENV_PLACEHOLDER): 
	@echo "Setting up virtual env for image creation"
	@docker build -t $(IMAGE_NAME) ./scripts
	@echo -e $(GEN_MSG) > $(ENV_PLACEHOLDER)

build_image: build_kernel build_blr build_drivers $(ENV_PLACEHOLDER)
	@echo "Starting image creation"
	@touch $(OUTPUT_DIR)/archis_os.iso
	$(RUN_DOCKER_SCRIPT)

$(OUTPUT_DIR):
	@mkdir -p $(OUTPUT_DIR)

build_blr: $(OUTPUT_DIR)
	@rustup target list --installed | grep -qx $(BLR_TARGET) || {\
		echo "Adding blr target configuration";\
		rustup target add $(BLR_TARGET);\
	}
	@echo "Building bootloader..." 
	@(cd $(BLR_CRATE_PATH) && \
		cargo build $(BUILD_OPTIONS) \
		-Z build-std=core,alloc \
		--target $(BLR_TARGET) \
	)
	@cp $(BLR_EXE) $(OUTPUT_DIR)/bootx64.efi

build_kernel: $(OUTPUT_DIR)
	@echo "Building kernel..."
	@rm -f $(KERN_PLACEHOLDER)
	@(cd kernel && RUSTFLAGS="-C link-arg=-T$(LINKER_SCRIPT)" \
		cargo build $(BUILD_OPTIONS) \
    	-Z build-std=core,compiler_builtins \
    	-Z build-std-features=compiler-builtins-mem \
    	--target $(KERNEL_TARGET) \
	) 
	@cp $(KERNEL_EXE) $(OUTPUT_DIR)/aris.elf

build_drivers: build_kernel
	@echo "Building drivers..."
	@mkdir -p $(OUTPUT_DIR)/drivers
	@set -e; for dir in $(DRIVER_DIRS); do \
		if [ -f $$dir/Cargo.toml ]; then \
			driver_name=$$(basename $$dir); \
			echo "Building driver $$dir"; \
			(cd $$dir && \
				RUSTFLAGS="-C link-arg=-T$(LINKER_SCRIPT) -C link-arg=-Ltarget/$(KERNEL_ARCH)/$(CONFIG)" \
				cargo build $(BUILD_OPTIONS) \
				-Z build-std=core,compiler_builtins \
				-Z build-std-features=compiler-builtins-mem \
				--target ../../../$(KERNEL_TARGET)); \
			cp target/$(KERNEL_ARCH)/$(CONFIG)/lib$$driver_name.so $(OUTPUT_DIR)/drivers; \
		fi \
	done

run_unit_test: build_kernel
	@echo -e $(GEN_MSG) > $(KERN_PLACEHOLDER) 
	@cargo test --manifest-path=boot/blr/Cargo.toml -- --nocapture
	@cargo test --manifest-path=kernel/Cargo.toml -- --nocapture

test:
	@echo "Starting simulator..."
	@qemu-system-x86_64 -cpu Nehalem -bios scripts/OVMF.fd\
 -drive file=$(OUTPUT_DIR)/archis_os.iso,format=raw,if=ide -serial mon:stdio | tee >(sed 's/\x1b\[[0-9;=]*[A-Za-z]//g' > $(OUTPUT_DIR)/con_log.txt)

clean:
	@echo "Cleaning all builds..."
	@cd $(BLR_CRATE_PATH) && cargo clean
	@cd $(KERNEL_CRATE_PATH) && cargo clean
	@rm -rf $(OUTPUT_DIR)

# Execute this to restart build process from very beginning
# Use this if facing some problems with build
reset: clean
	@echo "Removing placeholders"
	@rm -f $(KERN_PLACEHOLDER) $(ENV_PLACEHOLDER)
