#include <stdlib.h>

#include "bluetooth.h"
#include "util.h"

void reverse_ba2str(const bdaddr_t *ba, char *str) {
  int size = sizeof(ba->b);

  for (unsigned int position = 0; position < size; position++) {
    unsigned int string_position = position * 3;
    unit_to_hex_string(ba->b[position], &str[string_position]);
    str[string_position + 2] = (char)':';
  }

  str[(size * 3) - 1] = 0;
}

void reverse_str2ba(const char *str, bdaddr_t *ba) {
  if (bachk(str) < 0) {
    memory_set(ba, 0, sizeof(*ba));
    return;
  }

  const int max_ba       = 6;
  const int numeric_base = 16;
  for (int i = 0; i < max_ba; i++, str += 3) {
    ba->b[i] = strtol(str, NULL, numeric_base);
  }
}
