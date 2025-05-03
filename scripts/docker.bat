@echo off
docker run -it --privileged -v "%~dp0..":/workspace -w /workspace disk-tools ./scripts/create_image.sh