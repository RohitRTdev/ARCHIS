#include "acpi.h"

extern void AcpiOsPrintStr(const char* s);

#define PRINTF_BUF_SIZE 512

static void itoa_dec(int value, char *buf) {
    char tmp[16];
    int i = 0, j = 0;
    int is_neg = value < 0;
    if (is_neg) value = -value;
    do {
        tmp[i++] = '0' + (value % 10);
        value /= 10;
    } while (value && i < 15);
    if (is_neg) tmp[i++] = '-';
    while (i--) *buf++ = tmp[i];
    *buf = 0;
}

static void itoa_hex(unsigned int value, char *buf) {
    char tmp[16];
    int i = 0;
    do {
        int digit = value & 0xF;
        tmp[i++] = digit < 10 ? '0' + digit : 'a' + digit - 10;
        value >>= 4;
    } while (value && i < 15);
    *buf++ = '0'; *buf++ = 'x';
    while (i--) *buf++ = tmp[i];
    *buf = 0;
}

void AcpiOsVprintf(const char *fmt, va_list args) {
    char buf[PRINTF_BUF_SIZE];
    char *out = buf;

    for (; *fmt && (out - buf) < PRINTF_BUF_SIZE - 1; ++fmt) {
        if (*fmt == '%') {
            ++fmt;
            if (!*fmt) break;
            if (*fmt == 's') {
                const char *s = va_arg(args, const char*);
                while (*s && (out - buf) < PRINTF_BUF_SIZE - 1)
                    *out++ = *s++;
            } else if (*fmt == 'd') {
                int v = va_arg(args, int);
                char tmp[16];
                itoa_dec(v, tmp);
                for (char *t = tmp; *t && (out - buf) < PRINTF_BUF_SIZE - 1; ++t)
                    *out++ = *t;
            } else if (*fmt == 'x') {
                unsigned int v = va_arg(args, unsigned int);
                char tmp[18];
                itoa_hex(v, tmp);
                for (char *t = tmp; *t && (out - buf) < PRINTF_BUF_SIZE - 1; ++t)
                    *out++ = *t;
            } else if (*fmt == 'c') {
                char c = (char)va_arg(args, int);
                *out++ = c;
            } else {
                *out++ = '%';
                *out++ = *fmt;
            }
        } else {
            *out++ = *fmt;
        }
    }
    *out = 0;
    va_end(args);

    AcpiOsPrintStr(buf);
}

void AcpiOsPrintf(const char *fmt, ...) {
    va_list args;
    va_start(args, fmt);
    AcpiOsVprintf(fmt, args);
    va_end(args);
}