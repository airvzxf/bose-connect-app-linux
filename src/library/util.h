#ifndef HOME_WOLF_WORKSPACE_PROJECTS_BOSE_CONNECT_APP_LINUX_SRC_LIBRARY_UTIL_H
#define HOME_WOLF_WORKSPACE_PROJECTS_BOSE_CONNECT_APP_LINUX_SRC_LIBRARY_UTIL_H

int str_to_byte(const char *str, uint8_t *byte);

void str_copy(char *to, const char *from, int size);

void memory_copy(uint8_t *to, const uint8_t *from, int size);

void memory_set(bdaddr_t *target, uint8_t constant_byte, size_t size);

#endif
