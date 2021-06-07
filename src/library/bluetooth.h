#ifndef BOSE_CONNECT_APP_LINUX_SRC_LIBRARY_BLUETOOTH_H
#define BOSE_CONNECT_APP_LINUX_SRC_LIBRARY_BLUETOOTH_H

#include <bluetooth/bluetooth.h>

#define BT_ADDR_LEN 6

void reverse_ba2str(const bdaddr_t *ba, char *str);

void reverse_str2ba(const char *str, bdaddr_t *ba);

#endif
