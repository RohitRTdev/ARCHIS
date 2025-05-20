#!/bin/bash
set -e

# === CONFIGURATION ===
IMG=output/archis_os.iso
SIZE_MB=200
EFI_SIZE_MB=100
EFI_LABEL="EFI"
ROOT_LABEL="ARCHIS"
ROOT_UUID="9ffd2959-915c-479f-8787-1f9f701e1034"  # Custom partition UUID
KERNEL_MNT_POINT="/mnt/kernel"
BLR_MNT_POINT="/mnt/esp"
echo "Creating empty image file"
dd if=/dev/zero of=$IMG bs=1M count=$SIZE_MB > /dev/null 2>&1

# === CREATE GPT TABLE AND PARTITIONS ===
echo "Creating GPT partition (ESP+System)"
parted $IMG --script -- \
  mklabel gpt \
  mkpart ESP fat32 1MiB ${EFI_SIZE_MB}MiB \
  set 1 esp on \
  mkpart primary fat32 ${EFI_SIZE_MB}MiB 100%

LOOPp0=$(losetup -f)
losetup --offset 1048576 --sizelimit $((${EFI_SIZE_MB}*1024*1024)) ${LOOPp0} $IMG
LOOPp1=$(losetup -f)
losetup --offset $((${EFI_SIZE_MB}*1024*1024)) ${LOOPp1} $IMG

echo "Formatting partitions as FAT32"
# === FORMAT EFI PARTITION ===
mkfs.vfat -F32 -n $EFI_LABEL ${LOOPp0} > /dev/null

# === FORMAT ROOT PARTITION ===
mkfs.vfat -F32 -n $ROOT_LABEL ${LOOPp1} > /dev/null

install_kernel_image() {
    local src="output"
    local dst_kernel=$KERNEL_MNT_POINT
    local dst_blr=$BLR_MNT_POINT

    echo "Installing kernel and bootloader into image..."

    mkdir -p "$dst_kernel/sys/drivers"
    mkdir -p "$dst_blr/efi/boot"

    cp "$src"/drivers/*.so "$dst_kernel/sys/drivers/" || echo "No drivers found..."
    cp "$src"/aris.elf "$dst_kernel/sys/" || echo "Kernel not found..."
    cp "$src"/bootx64.efi "$dst_blr/efi/boot/" || echo "Bootloader not found..."
}

# === CREATE MOUNTPOINTS ===
mkdir -p /mnt/esp
mkdir -p /mnt/kernel
mount ${LOOPp0} /mnt/esp
mount ${LOOPp1} /mnt/kernel

install_kernel_image

# === CLEANUP ===
umount $BLR_MNT_POINT 
umount $KERNEL_MNT_POINT
losetup -d $LOOPp0
losetup -d $LOOPp1
rmdir $BLR_MNT_POINT
rmdir $KERNEL_MNT_POINT

echo "Setting partition UUID"

# === SET CUSTOM PARTITION UUID ===
echo -e "x\nc\n2\n${ROOT_UUID}\nw\ny\n" | gdisk $IMG > /dev/null 2>&1
echo "Bootable GPT image '$IMG' generated."