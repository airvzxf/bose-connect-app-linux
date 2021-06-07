#include "util.h"

#define CHAR_BIT 1

static uint8_t get_value(char c) {
  const int max_decimal_unit   = 10;
  const int ten_in_hexadecimal = 16;

  if ('0' <= c && c <= '9') {
    return (uint8_t)(c - '0');
  }

  if ('A' <= c && c <= 'F') {
    return (uint8_t)(c - 'A' + max_decimal_unit);
  }

  if ('a' <= c && c <= 'f') {
    return (uint8_t)(c - 'a' + max_decimal_unit);
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

void str_copy(char *to, const char *from, int size) {
  const int ascii_null_character  = 0;
  const int ascii_space_character = 32;
  to[size]                        = 0;

  for (unsigned int position = 0; position < size; position++) {

    if (from[position] == ascii_null_character) {
      to[position] = from[position];
      return;
    }

    if (from[position] > ascii_space_character ||
        from[position] < ascii_null_character) {
      to[position] = from[position];
    }
  }
}

void memory_copy(uint8_t *to, const uint8_t *from, int size) {
  for (unsigned int position = 0; position < size; position++) {
    to[position] = from[position];
  }
}

void memory_set(bdaddr_t *target, uint8_t constant_byte, size_t size) {
  for (unsigned int position = 0; position < size; position++) {
    target->b[position] = constant_byte;
  }
}

void unit_to_hex_string(int number, char *dest) {
  const int dest_size = 3;

  if (dest == NULL) {
    return;
  }

  if (number >= 0 && number < MAX_HEXADECIMAL_UNIT) {
    dest[0] = (char)'0';
    if (number < MAX_DECIMAL_UNIT) {
      dest[1] = (char)(ASCII_NUMBER_ZERO + number);
    } else {
      dest[1] = (char)(ASCII_LETTER_A + number - MAX_DECIMAL_UNIT);
    }
    return;
  }

  char  buffer[sizeof number * CHAR_BIT + 2];
  char *p = &buffer[sizeof buffer - 1];

  int neg_num = number < 0 ? number : -number;

  *p = '\0';
  do {
    *--p = "0123456789ABCDEF"[-(neg_num % MAX_HEXADECIMAL_UNIT)];
    neg_num /= MAX_HEXADECIMAL_UNIT;
  } while (neg_num);
  if (number < 0) {
    *--p = '-';
  }

  int src_size = (int)(&buffer[sizeof buffer] - p);
  if (src_size - 1 > dest_size) {
    return;
  }

  str_copy(dest, p, src_size);
}
