#!/bin/bash
set -e

# === CONFIGURATION ===
IMG=output/archis_os.iso
SIZE_MB=200
EFI_SIZE_MB=100
EFI_LABEL="EFI"
ROOT_LABEL="ARCHIS"
BLR="output/boot.efi"
KERNEL="output/aris.elf"  
ROOT_UUID="9ffd2959-915c-479f-8787-1f9f701e1034"  # Custom partition UUID

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


# === MOUNT AND COPY EFI FILE ===
mkdir -p /mnt/esp
mount ${LOOPp0} /mnt/esp

mkdir -p /mnt/esp/EFI/BOOT
cp "$BLR" /mnt/esp/EFI/BOOT/BOOTX64.EFI


# === COPY KERNEL FILE === 
mkdir -p /mnt/kernel
mount ${LOOPp1} /mnt/kernel

mkdir -p /mnt/kernel/sys/drivers
mkdir -p /mnt/kernel/bin
cp "$KERNEL" /mnt/kernel/sys/aris.elf

# === CLEANUP ===
umount /mnt/esp
umount /mnt/kernel
losetup -d $LOOPp0
losetup -d $LOOPp1
rmdir /mnt/esp
rmdir /mnt/kernel

echo "Setting partition UUID"

# === SET CUSTOM PARTITION UUID ===
echo -e "x\nc\n2\n${ROOT_UUID}\nw\ny\n" | gdisk $IMG > /dev/null 2>&1
echo "Bootable GPT image '$IMG' generated."