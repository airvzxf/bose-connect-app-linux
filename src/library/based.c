#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "based.h"

#define ANY              0x00
#define CN_BASE_PACK_LEN 4
#define MAX_NAME_PACKAGE (CN_BASE_PACK_LEN + MAX_NAME_LEN)
#define GET_DEVICE_ID_SEND                                                     \
  { 0x00, 0x03, 0x01, 0x00 }
#define GET_DEVICE_ID_ACK                                                      \
  { 0x00, 0x03, 0x03, 0x03 }
#define GET_NAME_ACK                                                           \
  { 0x01, 0x02, 0x03, ANY, 0x00 }
#define GET_NAME_MASK                                                          \
  { 0xff, 0xff, 0xff, 0x00, 0xff }
#define SET_NAME_SEND                                                          \
  { 0x01, 0x02, 0x02, ANY }
#define GET_PROMPT_LANGUAGE_ACK                                                \
  { 0x01, 0x03, 0x03, 0x05, ANY, 0x00, ANY, ANY, 0xde }
#define GET_PROMPT_LANGUAGE_MASK                                               \
  { 0xff, 0xff, 0xff, 0xff, 0x00, 0xff, 0x00, 0x00, 0xff }
#define SET_PROMPT_LANGUAGE_SEND                                               \
  { 0x01, 0x03, 0x02, 0x01, ANY }
#define GET_AUTO_OFF_ACK                                                       \
  { 0x01, 0x04, 0x03, 0x01, ANY }
#define GET_AUTO_OFF_MASK                                                      \
  { 0xff, 0xff, 0xff, 0xff, 0x00 }
#define SET_AUTO_OFF_SEND                                                      \
  { 0x01, 0x04, 0x02, 0x01, ANY }
#define GET_NOISE_CANCELLING_ACK                                               \
  { 0x01, 0x06, 0x03, 0x02, ANY, 0x0b }
#define GET_NOISE_CANCELLING_MASK                                              \
  { 0xff, 0xff, 0xff, 0xff, 0x00, 0xff }
#define SET_NOISE_CANCELLING_SEND                                              \
  { 0x01, 0x06, 0x02, 0x01, ANY }
#define NOISE_CANCELLING_14 0x4014
#define NOISE_CANCELLING_20 0x4020
#define NOISE_CANCELLING_0C 0x400c
#define GET_DEVICE_STATUS_SEND                                                 \
  { 0x01, 0x01, 0x05, 0x00 }
#define GET_DEVICE_STATUS_ACK                                                  \
  { 0x01, 0x01, 0x07, 0x00 }
#define GET_FIRMWARE_VERSION_SEND                                              \
  { 0x00, 0x05, 0x01, 0x00 }
#define GET_FIRMWARE_VERSION_ACK                                               \
  { 0x00, 0x05, 0x03, 0x05 }
#define GET_SERIAL_NUMBER_SEND                                                 \
  { 0x00, 0x07, 0x01, 0x00 }
#define GET_SERIAL_NUMBER_ACK                                                  \
  { 0x00, 0x07, 0x03 }
#define GET_BATTERY_LEVEL_SEND                                                 \
  { 0x02, 0x02, 0x01, 0x00 }
#define GET_BATTERY_LEVEL_ACK                                                  \
  { 0x02, 0x02, 0x03, 0x01 }
#define GET_PAIRED_DEVICES_SEND                                                \
  { 0x04, 0x04, 0x01, 0x00 }
#define GET_PAIRED_DEVICES_ACK                                                 \
  { 0x04, 0x04, 0x03 }
#define INIT_CONNECTION_SEND                                                   \
  { 0x00, 0x01, 0x01, 0x00 }
#define INIT_CONNECTION_ACK                                                    \
  { 0x00, 0x01, 0x03, 0x05 }
#define SET_PARING_SEND_PACKAGE                                                \
  { 0x04, 0x08, 0x05, 0x01, ANY }
#define SET_PARING_ACK_PACKAGE                                                 \
  { 0x04, 0x08, 0x06, 0x01, ANY }
#define SET_SELF_VOICE_SEND_PACKAGE                                            \
  { 0x01, 0x0b, 0x02, 0x02, 0x01, ANY, 0x38 }
#define SET_SELF_VOICE_ACK_PACKAGE                                             \
  { 0x01, 0x0b, 0x03, 0x03, 0x01, ANY, 0x0f }
#define GET_DEVICE_INFO_SEND_PACKAGE                                           \
  { 0x04, 0x05, 0x01, BT_ADDR_LEN }
#define GET_DEVICE_INFO_ACK_PACKAGE                                            \
  { 0x04, 0x05, 0x03 }
#define CONNECT_DEVICE_SEND                                                    \
  { 0x04, 0x01, 0x05, BT_ADDR_LEN + 1, 0x00 }
#define CONNECT_DEVICE_ACK                                                     \
  { 0x04, 0x01, 0x07, BT_ADDR_LEN }
#define DISCONNECT_DEVICE_SEND                                                 \
  { 0x04, 0x02, 0x05, BT_ADDR_LEN }
#define DISCONNECT_DEVICE_ACK                                                  \
  { 0x04, 0x02, 0x07, BT_ADDR_LEN }
#define REMOVE_DEVICE_SEND                                                     \
  { 0x04, 0x03, 0x05, BT_ADDR_LEN }
#define REMOVE_DEVICE_ACK                                                      \
  { 0x04, 0x03, 0x06, BT_ADDR_LEN }
#define MAX_BYTES_2  2
#define MAX_BYTES_3  3
#define MAX_BYTES_4  4
#define MAX_BYTES_5  5
#define MAX_BYTES_10 10
#define MAX_BYTES_11 11

int has_noise_cancelling(unsigned int device_id) {
  switch (device_id) {
  case NOISE_CANCELLING_14:
  case NOISE_CANCELLING_20:
  case NOISE_CANCELLING_0C:
    return 1;
  default:
    return 0;
  }
}

static int masked_memory_cmp(const uint8_t *ptr1, uint8_t *ptr2, size_t num,
                             const uint8_t *mask) {
  while (num-- > 0) {
    uint8_t mask_byte = *mask++;
    uint8_t byte1     = *ptr1++ & mask_byte;
    uint8_t byte2     = *ptr2++ & mask_byte;

    if (byte1 != byte2) {
      return byte1 - byte2;
    }
  }

  return 0;
}

static int read_check(int sock, uint8_t *receive, size_t receive_n,
                      const uint8_t *ack, const uint8_t *mask) {
  int status = read(sock, receive, receive_n);
  if (status != receive_n) {
    return status ? status : 1;
  }

  return abs(mask ? masked_memory_cmp(ack, receive, receive_n, mask)
                  : memcmp(ack, receive, receive_n));
}

static int write_check(int sock, const void *send, size_t send_n,
                       const void *ack, size_t ack_n) {
  uint8_t buffer[ack_n];

  int status = write(sock, send, send_n);
  if (status != send_n) {
    return status ? status : 1;
  }
  return read_check(sock, buffer, sizeof(buffer), ack, NULL);
}

int send_packet(int sock, const void *send, size_t send_n,
                uint8_t received[MAX_BT_PACK_LEN]) {
  int status = write(sock, send, send_n);
  if (status != send_n) {
    return status ? status : 1;
  }

  return read(sock, received, MAX_BT_PACK_LEN);
}

int init_connection(int sock) {
  static const uint8_t send[] = INIT_CONNECTION_SEND;
  static const uint8_t ack[]  = INIT_CONNECTION_ACK;

  int status = write_check(sock, send, sizeof(send), ack, sizeof(ack));
  if (status) {
    return status;
  }

  // Throw away the initial firmware version
  uint8_t garbage[MAX_BYTES_5];
  status = read(sock, garbage, sizeof(garbage));

  if (status != sizeof(garbage)) {
    return status ? status : 1;
  }

  return 0;
}

int get_device_id(int sock, unsigned int *device_id, unsigned int *index) {
  static const uint8_t send[] = GET_DEVICE_ID_SEND;
  static const uint8_t ack[]  = GET_DEVICE_ID_ACK;

  int status = write_check(sock, send, sizeof(send), ack, sizeof(ack));
  if (status) {
    return status;
  }

  uint16_t device_id_half_word = 0;
  status = read(sock, &device_id_half_word, sizeof(device_id_half_word));
  if (status != sizeof(device_id_half_word)) {
    return status ? status : 1;
  }

  *device_id = bswap_16(device_id_half_word);

  uint8_t index_byte = 0;
  status             = read(sock, &index_byte, 1);
  if (status != 1) {
    return status ? status : 1;
  }
  *index = index_byte;

  return 0;
}

static int get_name(int sock, char name[MAX_NAME_LEN + 1]) {
  static const uint8_t ack[]  = GET_NAME_ACK;
  static const uint8_t mask[] = GET_NAME_MASK;
  uint8_t              buffer[sizeof(ack)];

  int status = read_check(sock, buffer, sizeof(buffer), ack, mask);
  if (status) {
    return status;
  }

  size_t length = buffer[MAX_BYTES_3] - 1;
  status        = read(sock, name, length);
  if (status != length) {
    return status ? status : 1;
  }
  name[length] = '\0';

  return 0;
}

int set_name(int sock, const char *name) {
  static uint8_t send[MAX_NAME_PACKAGE] = SET_NAME_SEND;
  size_t         length                 = strlen(name);

  send[MAX_BYTES_3] = length;
  strncpy((char *)&send[CN_BASE_PACK_LEN], name, MAX_NAME_LEN);

  size_t send_size = CN_BASE_PACK_LEN + length;
  int    status    = write(sock, send, send_size);
  if (status != send_size) {
    return status ? status : 1;
  }

  char got_name[MAX_NAME_LEN + 1];
  status = get_name(sock, got_name);
  if (status) {
    return status;
  }

  return abs(strcmp(name, got_name));
}

static int get_prompt_language(int sock, enum PromptLanguage *language) {
  // TODO(wolf): ensure that this value is correct
  // TODO(wolf): figure out what bytes 6 and 7 are for
  static const uint8_t ack[]  = GET_PROMPT_LANGUAGE_ACK;
  static const uint8_t mask[] = GET_PROMPT_LANGUAGE_MASK;
  uint8_t              buffer[sizeof(ack)];

  int status = read_check(sock, buffer, sizeof(buffer), ack, mask);
  if (status) {
    return status;
  }

  *language = buffer[MAX_BYTES_4];
  return 0;
}

int set_prompt_language(int sock, enum PromptLanguage language) {
  static uint8_t send[] = SET_PROMPT_LANGUAGE_SEND;
  send[MAX_BYTES_4]     = language;

  int status = write(sock, send, sizeof(send));
  if (status != sizeof(send)) {
    return status ? status : 1;
  }

  enum PromptLanguage got_language = 0x0;
  status                           = get_prompt_language(sock, &got_language);
  if (status) {
    return status;
  }

  return (int)(language - got_language);
}

int set_voice_prompts(int sock, int on) {
  char                 name[MAX_NAME_LEN + 1];
  enum PromptLanguage  pl = 0x0;
  enum AutoOff         ao = 0x0;
  enum NoiseCancelling nc = 0x0;

  int status = get_device_status(sock, name, &pl, &ao, &nc);
  if (status) {
    return status;
  }

  if (on) {
    pl |= VP_MASK;
  } else {
    pl &= ~VP_MASK;
  }

  return set_prompt_language(sock, pl);
}

static int get_auto_off(int sock, enum AutoOff *minutes) {
  static const uint8_t ack[]  = GET_AUTO_OFF_ACK;
  static const uint8_t mask[] = GET_AUTO_OFF_MASK;
  uint8_t              buffer[sizeof(ack)];

  int status = read_check(sock, buffer, sizeof(buffer), ack, mask);
  if (status) {
    return status;
  }

  *minutes = buffer[MAX_BYTES_4];
  return 0;
}

int set_auto_off(int sock, enum AutoOff minutes) {
  static uint8_t send[] = SET_AUTO_OFF_SEND;
  send[MAX_BYTES_4]     = minutes;

  int status = write(sock, send, sizeof(send));
  if (status != sizeof(send)) {
    return status ? status : 1;
  }

  enum AutoOff got_minutes = 0;
  status                   = get_auto_off(sock, &got_minutes);
  if (status) {
    return status;
  }

  return (int)(minutes - got_minutes);
}

static int get_noise_cancelling(int sock, enum NoiseCancelling *level) {
  static const uint8_t ack[]  = GET_NOISE_CANCELLING_ACK;
  static const uint8_t mask[] = GET_NOISE_CANCELLING_MASK;
  uint8_t              buffer[sizeof(ack)];

  int status = read_check(sock, buffer, sizeof(buffer), ack, mask);
  if (status) {
    return status;
  }

  *level = buffer[MAX_BYTES_4];
  return 0;
}

int set_noise_cancelling(int sock, enum NoiseCancelling level) {
  static uint8_t send[] = SET_NOISE_CANCELLING_SEND;
  send[MAX_BYTES_4]     = level;

  int status = write(sock, send, sizeof(send));
  if (status != sizeof(send)) {
    return status ? status : 1;
  }

  enum NoiseCancelling got_level = 0;
  status                         = get_noise_cancelling(sock, &got_level);
  if (status) {
    return status;
  }

  return (int)(level - got_level);
}

int get_device_status(int sock, char name[MAX_NAME_LEN + 1],
                      enum PromptLanguage *language, enum AutoOff *minutes,
                      enum NoiseCancelling *level) {
  unsigned int device_id = 0;
  unsigned int index     = 0;
  int          status    = get_device_id(sock, &device_id, &index);
  if (status) {
    return status;
  }
  static const uint8_t send[] = GET_DEVICE_STATUS_SEND;
  status                      = write(sock, send, sizeof(send));
  if (status != sizeof(send)) {
    return status ? status : 1;
  }

  static const uint8_t ack[] = GET_DEVICE_STATUS_ACK;
  uint8_t              buffer1[sizeof(ack)];

  status = read_check(sock, buffer1, sizeof(buffer1), ack, NULL);
  if (status) {
    return status;
  }

  status = get_name(sock, name);
  if (status) {
    return status;
  }

  status = get_prompt_language(sock, language);
  if (status) {
    return status;
  }

  status = get_auto_off(sock, minutes);
  if (status) {
    return status;
  }

  if (has_noise_cancelling(device_id)) {
    status = get_noise_cancelling(sock, level);
    if (status) {
      return status;
    }
  } else {
    *level = NC_DNE;
  }

  return status;
}

int set_pairing(int sock, enum Pairing pairing) {
  static uint8_t send[] = SET_PARING_SEND_PACKAGE;
  static uint8_t ack[]  = SET_PARING_ACK_PACKAGE;
  send[MAX_BYTES_4]     = pairing;
  ack[MAX_BYTES_4]      = pairing;
  return write_check(sock, send, sizeof(send), ack, sizeof(ack));
}

int set_self_voice(int sock, enum SelfVoice selfVoice) {
  static uint8_t send[] = SET_SELF_VOICE_SEND_PACKAGE;
  static uint8_t ack[]  = SET_SELF_VOICE_ACK_PACKAGE;

  send[MAX_BYTES_5] = selfVoice;
  ack[MAX_BYTES_5]  = selfVoice;
  return write_check(sock, send, sizeof(send), ack, sizeof(ack));
}

int get_firmware_version(int sock, char version[VER_STR_LEN]) {
  static const uint8_t send[] = GET_FIRMWARE_VERSION_SEND;
  static const uint8_t ack[]  = GET_FIRMWARE_VERSION_ACK;

  int status = write_check(sock, send, sizeof(send), ack, sizeof(ack));
  if (status) {
    return status;
  }

  status = read(sock, version, VER_STR_LEN - 1);
  if (status != VER_STR_LEN - 1) {
    return status ? status : 1;
  }

  version[VER_STR_LEN - 1] = '\0';
  return 0;
}

int get_serial_number(int sock, char serial[MAX_SERIAL_SIZE]) {
  static const uint8_t send[] = GET_SERIAL_NUMBER_SEND;
  static const uint8_t ack[]  = GET_SERIAL_NUMBER_ACK;

  int status = write_check(sock, send, sizeof(send), ack, sizeof(ack));
  if (status) {
    return status;
  }

  uint8_t length = 0;
  status         = read(sock, &length, 1);
  if (status != 1) {
    return status ? status : 1;
  }

  status = read(sock, serial, length);
  if (status != length) {
    return status ? status : 1;
  }
  serial[length] = '\0';

  return 0;
}

int get_battery_level(int sock, unsigned int *level) {
  static const uint8_t send[] = GET_BATTERY_LEVEL_SEND;
  static const uint8_t ack[]  = GET_BATTERY_LEVEL_ACK;

  int status = write_check(sock, send, sizeof(send), ack, sizeof(ack));
  if (status) {
    return status;
  }

  uint8_t level_byte = 0;
  read(sock, &level_byte, 1);
  *level = level_byte;

  return 0;
}

int get_device_info(int sock, bdaddr_t address, struct Device *device) {
  static uint8_t       send[MAX_BYTES_10] = GET_DEVICE_INFO_SEND_PACKAGE;
  static const uint8_t ack[]              = GET_DEVICE_INFO_ACK_PACKAGE;

  memcpy(&send[MAX_BYTES_4], &address.b, BT_ADDR_LEN);

  int status = write_check(sock, send, sizeof(send), ack, sizeof(ack));
  if (status) {
    return status;
  }

  uint8_t length = 0;
  status         = read(sock, &length, 1);
  if (status != 1) {
    return status ? status : 1;
  }

  status = read(sock, &device->address.b, BT_ADDR_LEN);
  if (status != BT_ADDR_LEN) {
    return status ? status : 1;
  }
  length -= BT_ADDR_LEN;

  status = memcmp(&address.b, &device->address.b, BT_ADDR_LEN);
  if (status) {
    return abs(status);
  }

  uint8_t status_byte = 0;
  status              = read(sock, &status_byte, 1);
  if (status != 1) {
    return status ? status : 1;
  }
  length -= 1;

  device->status = status_byte;

  // TODO(wolf): figure out what the first byte of garbage is for
  uint8_t garbage[MAX_BYTES_2];
  status = read(sock, &garbage, sizeof(garbage));
  if (status != sizeof(garbage)) {
    return status ? status : 1;
  }
  length -= sizeof(garbage);

  status = read(sock, device->name, length);
  if (status != length) {
    return status ? status : 1;
  }
  device->name[length] = '\0';

  return 0;
}

int get_paired_devices(int sock, bdaddr_t addresses[MAX_NUM_DEVICES],
                       size_t *num_devices, enum DevicesConnected *connected) {
  static const uint8_t send[] = GET_PAIRED_DEVICES_SEND;
  static const uint8_t ack[]  = GET_PAIRED_DEVICES_ACK;

  int status = write_check(sock, send, sizeof(send), ack, sizeof(ack));
  if (status) {
    return status;
  }

  uint8_t num_devices_byte = 0;
  status                   = read(sock, &num_devices_byte, 1);
  if (status != 1) {
    return status ? status : 1;
  }

  num_devices_byte /= BT_ADDR_LEN;
  *num_devices = num_devices_byte - 1;

  uint8_t num_connected_byte = 0;
  status                     = read(sock, &num_connected_byte, 1);
  if (status != 1) {
    return status ? status : 1;
  }
  *connected = num_connected_byte;

  for (size_t i = 0; i < num_devices_byte; ++i) {
    status = read(sock, &addresses[i].b, BT_ADDR_LEN);
    if (status != BT_ADDR_LEN) {
      return status ? status : 1;
    }
  }

  return 0;
}

int connect_device(int sock, bdaddr_t address) {
  static uint8_t send[MAX_BYTES_11] = CONNECT_DEVICE_SEND;
  static uint8_t ack[MAX_BYTES_10]  = CONNECT_DEVICE_ACK;
  memcpy(&send[MAX_BYTES_5], &address.b, BT_ADDR_LEN);
  memcpy(&ack[MAX_BYTES_4], &address.b, BT_ADDR_LEN);
  return write_check(sock, send, sizeof(send), ack, sizeof(ack));
}

int disconnect_device(int sock, bdaddr_t address) {
  static uint8_t send[MAX_BYTES_10] = DISCONNECT_DEVICE_SEND;
  static uint8_t ack[MAX_BYTES_10]  = DISCONNECT_DEVICE_ACK;
  memcpy(&send[MAX_BYTES_4], &address.b, BT_ADDR_LEN);
  memcpy(&ack[MAX_BYTES_4], &address.b, BT_ADDR_LEN);
  return write_check(sock, send, sizeof(send), ack, sizeof(ack));
}

int remove_device(int sock, bdaddr_t address) {
  static uint8_t send[MAX_BYTES_10] = REMOVE_DEVICE_SEND;
  static uint8_t ack[MAX_BYTES_10]  = REMOVE_DEVICE_ACK;
  memcpy(&send[MAX_BYTES_4], &address.b, BT_ADDR_LEN);
  memcpy(&ack[MAX_BYTES_4], &address.b, BT_ADDR_LEN);
  return write_check(sock, send, sizeof(send), ack, sizeof(ack));
}
