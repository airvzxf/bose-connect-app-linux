#include "bluetooth.h"
#include <stdlib.h>

int reverse_ba2str(const bdaddr_t *ba, char *str) {
  return sprintf(str, "%2.2X:%2.2X:%2.2X:%2.2X:%2.2X:%2.2X", ba->b[0], ba->b[1],
                 ba->b[2], ba->b[3], ba->b[4], ba->b[5]);
}

int reverse_str2ba(const char *str, bdaddr_t *ba) {
  if (bachk(str) < 0) {
    memset(ba, 0, sizeof(*ba));
    return -1;
  }

  const int max_ba       = 6;
  const int numeric_base = 16;
  for (int i = 0; i < max_ba; i++, str += 3) {
    ba->b[i] = strtol(str, NULL, numeric_base);
  }

  return 0;
}
