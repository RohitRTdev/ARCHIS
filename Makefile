CONFIG = debug
BLR_TARGET = x86_64-unknown-uefi
BLR_TARGET_PLACEHOLDER = boot/uefi/placeholder.txt
ENV_PLACEHOLDER = placeholder.txt
KERNEL_TARGET = config/x86_64/x86_64.json
BLR_CRATE_PATH = boot/uefi
KERNEL_CRATE_PATH = kernel
BLR_EXE = $(BLR_CRATE_PATH)/target/$(BLR_TARGET)/$(CONFIG)/boot.efi
KERNEL_EXE = $(KERNEL_CRATE_PATH)/target/x86_64/$(CONFIG)/aris
OUTPUT_DIR = output
IMAGE_NAME=disk-tools
GEN_MSG = "Automatically generated file..\nDo not remove file manually.."

ifeq ($(OS),Windows_NT)
    RUN_DOCKER_SCRIPT = @./scripts/docker.bat
else
    RUN_DOCKER_SCRIPT = @echo "Not Windows, skipping batch script."
endif


.PHONY: all clean build_blr build_kernel build_image

# Default build
all: build_image

$(ENV_PLACEHOLDER): 
	@echo "Setting up virtual env for image creation"
	@docker build -t $(IMAGE_NAME) ./scripts
	@echo -e $(GEN_MSG) > $(ENV_PLACEHOLDER)

build_image: build_kernel build_blr $(ENV_PLACEHOLDER)
	@echo "Starting image creation"
	$(RUN_DOCKER_SCRIPT)

$(OUTPUT_DIR):
	@mkdir -p $(OUTPUT_DIR)

# Rule to build the UEFI crate
build_blr: $(BLR_TARGET_PLACEHOLDER) $(OUTPUT_DIR)
	@echo "Building boot/uefi crate..."
	@cd $(BLR_CRATE_PATH) && cargo build --target $(BLR_TARGET)
	@cp $(BLR_EXE) $(OUTPUT_DIR)

$(BLR_TARGET_PLACEHOLDER):
	@echo "Adding blr target configuration"
	@rustup target add $(BLR_TARGET)
	@echo -e $(GEN_MSG) > $(BLR_TARGET_PLACEHOLDER)

build_kernel: $(OUTPUT_DIR)
	@echo "Building kernel crate..."
	@cd kernel && cargo build --target $(KERNEL_TARGET) 
	@cp $(KERNEL_EXE) $(OUTPUT_DIR)

test:
	@echo "Starting simulator..."
	@qemu-system-x86_64 -cpu Nehalem -bios scripts/OVMF.fd\
 -drive file=$(OUTPUT_DIR)/archis_os.iso,format=raw,if=ide -serial mon:stdio | tee >(sed 's/\x1b\[[0-9;=]*[A-Za-z]//g' > output/con_log.txt)

# Clean all builds
clean:
	@echo "Cleaning all builds..."
	@cd $(BLR_CRATE_PATH) && cargo clean
	@cd $(KERNEL_CRATE_PATH) && cargo clean
	@rm -rf $(OUTPUT_DIR)

reset: clean
	@echo "Removing placeholders"
	@rm -f $(BLR_TARGET_PLACEHOLDER) $(ENV_PLACEHOLDER)