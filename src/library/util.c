#include <stdint.h>

static uint8_t get_value(char c) {
  const int max_decimal_unit   = 10;
  const int ten_in_hexadecimal = 16;

  if ('0' <= c && c <= '9') {
    return c - '0';
  }

  if ('A' <= c && c <= 'F') {
    return c - 'A' + max_decimal_unit;
  }

  if ('a' <= c && c <= 'f') {
    return c - 'a' + max_decimal_unit;
  }

  return ten_in_hexadecimal;
}

int str_to_byte(const char *str, uint8_t *byte) {
  const int max_hexadecimal_unit = 15;
  const int ten_hexadecimal      = 0x10;
  uint8_t   total                = 0;

  uint8_t part = get_value(str[0]);
  if (part > max_hexadecimal_unit) {
    return 1;
  }
  total += ten_hexadecimal * part;

  part = get_value(str[1]);
  if (part > max_hexadecimal_unit) {
    return 1;
  }
  total += part;

  *byte = total;

  return 0;
}
