#ifndef HOME_WOLF_WORKSPACE_PROJECTS_BOSE_CONNECT_APP_LINUX_SRC_LIBRARY_BLUETOOTH_H
#define HOME_WOLF_WORKSPACE_PROJECTS_BOSE_CONNECT_APP_LINUX_SRC_LIBRARY_BLUETOOTH_H

#include <bluetooth/bluetooth.h>

#define BT_ADDR_LEN 6

int reverse_ba2str(const bdaddr_t *ba, char *str);

int reverse_str2ba(const char *str, bdaddr_t *ba);

#endif
