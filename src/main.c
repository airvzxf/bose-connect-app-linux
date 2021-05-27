#include <bluetooth/bluetooth.h>
#include <bluetooth/rfcomm.h>
#include <getopt.h>
#include <stdlib.h>
#include <unistd.h>

#include "library/based.h"
#include "library/util.h"
#include "main.h"

#define OPTION_DEVICE_ID         5
#define OPTION_CONNECT_DEVICE    2
#define OPTION_DISCONNECT_DEVICE 3
#define OPTION_REMOVE_DEVICE     4
#define OPTION_SEND_PACKET       1

static void usage() {
  const char *message =
      "Usage: %s [options] <address>\n"

      "\t-h, --help\n"
      "\t\tPrint the help message.\n"

      "\t-i, --info\n"
      "\t\tPrint all the device information.\n"

      "\t-d, --device-status\n"
      "\t\tPrint the device status information. This includes its name,"
      " language,\n"
      "\t\tvoice-prompts, auto-off and noise cancelling settings.\n"

      "\t-f, --firmware-version\n"
      "\t\tPrint the firmware version on the device.\n"

      "\t-s, --serial-number\n"
      "\t\tPrint the serial number of the device.\n"

      "\t-b, --battery-level\n"
      "\t\tPrint the battery level of the device as a percent.\n"

      "\t-a, --paired-devices\n"
      "\t\tPrint the devices currently connected to the device.\n"
      "\t\t!: indicates the current device\n"
      "\t\t*: indicates other connected devices\n"

      "\t--device-id\n"
      "\t\tPrint the device id followed by the index revision.\n"

      "\t-n <name>, --name=<name>\n"
      "\t\tChange the name of the device.\n"

      "\t-o <minutes>, --auto-off=<minutes>\n"
      "\t\tChange the auto-off time.\n"
      "\t\tminutes: never, 5, 20, 40, 60, 180\n"

      "\t-c <level>, --noise-cancelling=<level>\n"
      "\t\tChange the noise cancelling level.\n"
      "\t\tlevel: high, low, off\n"

      "\t-l <language>, --prompt-language=<language>\n"
      "\t\tChange the voice-prompt language.\n"
      "\t\tlanguage: en, fr, it, de, es, pt, zh, ko, nl, ja, sv\n"

      "\t-v <switch>, --voice-prompts=<switch>\n"
      "\t\tChange whether voice-prompts are on or off.\n"
      "\t\tswitch: on, off\n"

      "\t-p <status>, --pairing=<status>\n"
      "\t\tChange whether the device is pairing.\n"
      "\t\tstatus: on, off\n"

      "\t-e, --self-voice=<level>\n"
      "\t\tChange the self voice level.\n"
      "\t\tlevel: high, medium, low, off\n"

      "\t--connect-device=<address>\n"
      "\t\tAttempt to connect to the device at address.\n"

      "\t--disconnect-device=<address>\n"
      "\t\tDisconnect the device at address.\n"

      "\t--remove-device=<address>\n"
      "\t\tRemove the device at address from the pairing list.\n";

  printf(message, PROGRAM_NAME);
}

int do_get_information(char *address) {
  while (do_get_device_id(address)) {
    sleep(4);
  }
  while (do_get_serial_number(address)) {
    sleep(4);
  }
  while (do_get_firmware_version(address)) {
    sleep(4);
  }
  while (do_get_battery_level(address)) {
    sleep(4);
  }
  while (do_get_device_status(address)) {
    sleep(4);
  }
  while (do_get_paired_devices(address)) {
    sleep(4);
  }

  return 0;
}

static int do_set_name(char *address, const char *arg) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }

  int status = 1;
  if (strlen(arg) > MAX_NAME_LEN) {
    fprintf(stderr, "Name exceeds %d character maximum. Truncating.\n",
            MAX_NAME_LEN);
  } else {
    char name_buffer[MAX_NAME_LEN + 1] = {0};
    strncpy(name_buffer, arg, MAX_NAME_LEN);
    status = set_name(sock, name_buffer);
  }

  close(sock);
  return status;
}

static int do_set_prompt_language(char *address, const char *arg) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }

  enum PromptLanguage pl = get_language(arg);

  if (pl == PL_UNKNOWN) {
    fprintf(stderr, "Invalid prompt language argument: %s\n", arg);
    usage();
    return 1;
  }

  const int status = set_prompt_language(sock, pl);
  close(sock);
  return status;
}

int get_voice_status(const char *arg) {
  if (strcmp(arg, "on") == 0) {
    return 1;
  }

  if (strcmp(arg, "off") == 0) {
    return 0;
  }

  return -1;
}

static int do_set_voice_prompts(char *address, const char *arg) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }

  int voice_status = get_voice_status(arg);

  if (voice_status == -1) {
    fprintf(stderr, "Invalid voice prompt argument: %s\n", arg);
    usage();
    return 1;
  }

  const int status = set_voice_prompts(sock, voice_status);
  close(sock);
  return status;
}

static int do_set_auto_off(char *address, const char *arg) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }

  int parsed = atoi(arg);

  enum AutoOff ao = AO_NEVER;
  switch (parsed) {
  case AO_5_MIN:
  case AO_20_MIN:
  case AO_40_MIN:
  case AO_60_MIN:
  case AO_180_MIN:
    ao = parsed;
    break;
  default:
    if (strcmp(arg, "never") != 0) {
      fprintf(stderr, "Invalid auto-off argument: %s\n", arg);
      usage();
      return 1;
    }
  }

  close(sock);
  return set_auto_off(sock, ao);
}

int get_noise_cancelling(const char *arg) {
  if (strcmp(arg, "high") == 0) {
    return NC_HIGH;
  }

  if (strcmp(arg, "low") == 0) {
    return NC_LOW;
  }

  if (strcmp(arg, "off") == 0) {
    return NC_OFF;
  }

  return NC_UNKNOWN;
}

static int do_set_noise_cancelling(char *address, const char *arg) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }
  enum NoiseCancelling nc = get_noise_cancelling(arg);

  if (nc == NC_UNKNOWN) {
    fprintf(stderr, "Invalid noise cancelling argument: %s\n", arg);
    usage();
    return 1;
  }

  unsigned int device_id = 0;
  unsigned int index     = 0;
  int          status    = get_device_id(sock, &device_id, &index);
  if (status) {
    return status;
  }

  if (!has_noise_cancelling(device_id)) {
    fprintf(stderr, "This device does not have noise cancelling.\n");
    usage();
    return 1;
  }

  status = set_noise_cancelling(sock, nc);
  close(sock);
  return status;
}

char *get_language_string(enum PromptLanguage language) {
  if (language == PL_EN) {
    return "EN";
  }

  if (language == PL_FR) {
    return "FR";
  }

  if (language == PL_IT) {
    return "IT";
  }

  if (language == PL_DE) {
    return "DE";
  }

  if (language == PL_ES) {
    return "ES";
  }

  if (language == PL_PT) {
    return "PT";
  }

  if (language == PL_ZH) {
    return "ZH";
  }

  if (language == PL_KO) {
    return "KO";
  }

  if (language == PL_NL) {
    return "NL";
  }

  if (language == PL_JA) {
    return "JA";
  }

  if (language == PL_SV) {
    return "SV";
  }

  if (language == PL_RU) {
    return "RU";
  }

  if (language == PL_PL) {
    return "PL";
  }

  return "";
}

static int do_get_device_status(char *address) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }

  printf("Status:\n");
  char                 name[MAX_NAME_LEN + 1];
  enum PromptLanguage  promptLanguage  = 0x0;
  enum AutoOff         autoOff         = 0x0;
  enum NoiseCancelling noiseCancelling = 0x0;

  int status = get_device_status(sock, name, &promptLanguage, &autoOff,
                                 &noiseCancelling);
  if (status) {
    return status;
  }

  printf("\tName: %s\n", name);

  char *    language             = NULL;
  const int max_unknown_language = 15;
  char      unknown_language[max_unknown_language];

  language = get_language_string((promptLanguage & VP_MASK));

  if (strcmp("", language) == 0) {
    sprintf(unknown_language, "Unknown [0x%02X]", promptLanguage);
    language = unknown_language;
  }

  printf("\tLanguage: %s\n", language);
  printf("\tVoice Prompts: %s\n", (promptLanguage & VP_MASK) ? "on" : "off");

  printf("\tAuto-Off: ");
  if (autoOff) {
    printf("%d", autoOff);
  } else {
    printf("never");
  }
  printf("\n");

  char *cancellingLevel = NULL;
  if (noiseCancelling != NC_DNE) {
    switch (noiseCancelling) {
    case NC_HIGH:
      cancellingLevel = "high";
      break;
    case NC_LOW:
      cancellingLevel = "low";
      break;
    case NC_OFF:
      cancellingLevel = "off";
      break;
    default:
      return 1;
    }
    printf("\tNoise Cancelling: %s\n", cancellingLevel);
  }

  close(sock);
  return 0;
}

int get_paring_status(const char *arg) {
  if (strcmp(arg, "on") == 0) {
    return P_ON;
  }

  if (strcmp(arg, "off") == 0) {
    return P_OFF;
  }

  return P_UNKNOWN;
}

static int do_set_pairing(char *address, const char *arg) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }
  enum Pairing p = get_paring_status(arg);

  if (p == P_UNKNOWN) {
    fprintf(stderr, "Invalid pairing argument: %s\n", arg);
    usage();
    return 1;
  }

  const int status = set_pairing(sock, p);
  close(sock);
  return status;
}

int get_self_voice_status(const char *arg) {
  if (strcmp(arg, "high") == 0) {
    return SV_HIGH;
  }

  if (strcmp(arg, "medium") == 0) {
    return SV_MEDIUM;
  }

  if (strcmp(arg, "low") == 0) {
    return SV_LOW;
  }

  if (strcmp(arg, "off") == 0) {
    return SV_OFF;
  }

  return SV_UNKNOWN;
}

static int do_set_self_voice(char *address, const char *arg) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }
  enum SelfVoice p = get_self_voice_status(arg);

  if (p == SV_UNKNOWN) {
    fprintf(stderr, "Invalid self voice argument: %s\n", arg);
    usage();
    return 1;
  }

  const int status = set_self_voice(sock, p);
  close(sock);
  return status;
}

static int do_get_firmware_version(char *address) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }
  printf("Firmware version: ");
  char version[VER_STR_LEN];
  int  status = get_firmware_version(sock, version);

  if (status) {
    return status;
  }

  printf("%s\n", version);

  close(sock);
  return 0;
}

static int do_get_serial_number(char *address) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }
  printf("Serial number: ");
  char serial[MAX_SERIAL_SIZE];
  int  status = get_serial_number(sock, serial);

  if (status) {
    return status;
  }

  printf("%s\n", serial);

  close(sock);
  return 0;
}

static int do_get_battery_level(char *address) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }
  printf("Battery level: ");
  unsigned int level  = 0;
  int          status = get_battery_level(sock, &level);

  if (status) {
    return status;
  }

  printf("%u\n", level);

  close(sock);
  return 0;
}

int get_paired_devices_connected(enum DevicesConnected connected) {
  if (connected == DC_ONE) {
    return 1;
  }
  if (connected == DC_TWO) {
    return 2;
  }

  return -1;
}

char get_paired_device_status(enum DeviceStatus status) {
  if (status == DS_THIS) {
    return '!';
  }

  if (status == DS_CONNECTED) {
    return '*';
  }

  if (status == DS_DISCONNECTED) {
    return ' ';
  }

  return ':';
}
static int do_get_paired_devices(char *address) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }

  printf("Paired devices: ");
  bdaddr_t              devices[MAX_NUM_DEVICES];
  size_t                num_devices = 0;
  enum DevicesConnected connected   = 0;

  int status = get_paired_devices(sock, devices, &num_devices, &connected);
  if (status) {
    return status;
  }

  if (connected == DC_UNKNOWN) {
    printf("\n\t"
           "Error: 0x%02X connected devices. Outside of the range "
           "(0x01 and 0x03)."
           "\n",
           connected);
    return 1;
  }

  unsigned int num_connected = get_paired_devices_connected(connected);

  printf("%zu\n", num_devices);
  printf("\tConnected: %u\n", num_connected);

  for (size_t i = 0; i < num_devices; ++i) {
    struct Device device;
    status = get_device_info(sock, devices[i], &device);
    if (status) {
      return status;
    }

    const int max_address_convert = 18;
    char      address_converted[max_address_convert];
    reverse_ba2str(&device.address, address_converted);

    char status_symbol = get_paired_device_status(device.status);

    if (status_symbol == ':') {
      return 1;
    }

    printf("\tDevice: %c | %s | %s\n", status_symbol, address_converted,
           device.name);
  }

  printf("\t[!] Indicates the current device.\n");
  printf("\t[*] Indicates other connected devices.\n");

  close(sock);
  return 0;
}

static int do_connect_device(char *address, const char *arg) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }

  bdaddr_t bd_address;
  reverse_str2ba(arg, &bd_address);
  int connection = connect_device(sock, bd_address);

  close(sock);
  return connection;
}

static int do_disconnect_device(char *address, const char *arg) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }
  bdaddr_t bd_address;
  reverse_str2ba(arg, &bd_address);
  int disconnection = disconnect_device(sock, bd_address);

  close(sock);
  return disconnection;
}

static int do_remove_device(char *address, const char *arg) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }
  bdaddr_t bd_address;
  reverse_str2ba(arg, &bd_address);
  int removed = remove_device(sock, bd_address);

  close(sock);
  return removed;
}

static int do_get_device_id(char *address) {
  int sock = get_socket(address);
  if (sock == -1) {
    return 1;
  }
  printf("Device ID: ");
  unsigned int device_id = 0;
  unsigned int index     = 0;
  int          status    = get_device_id(sock, &device_id, &index);

  if (status) {
    return status;
  }

  printf("0x%04x | Index: %u\n", device_id, index);

  close(sock);
  return 0;
}

static int do_send_packet(char *address, const char *arg) {
  int char_type_pointer_size = sizeof(char *);
  int sock                   = get_socket(address);

  if (sock == -1) {
    return 1;
  }

  uint8_t send[char_type_pointer_size / 2];
  for (size_t i = 0; arg[i * 2]; ++i) {
    if (str_to_byte(&arg[i * 2], &send[i]) != 0) {
      return 1;
    }
  }

  uint8_t received[MAX_BT_PACK_LEN];
  int     received_n = send_packet(sock, send, sizeof(send), received);
  if (received_n < 0) {
    return received_n;
  }

  printf("Received package:\n\t");
  for (size_t i = 0; i < received_n; ++i) {
    printf("%02x ", received[i]);
  }
  printf("\n");

  close(sock);
  return 0;
}

int get_socket(char *address) {
  static const struct timeval send_timeout    = {5, 0};
  static const struct timeval receive_timeout = {1, 0};
  int sock = socket(AF_BLUETOOTH, SOCK_STREAM, BTPROTO_RFCOMM);

  if (setsockopt(sock, SOL_SOCKET, SO_SNDTIMEO, &send_timeout,
                 sizeof(send_timeout)) < 0) {
    perror("Could not set socket send timeout");
    close(sock);
    return -1;
  }

  if (setsockopt(sock, SOL_SOCKET, SO_RCVTIMEO, &receive_timeout,
                 sizeof(receive_timeout)) < 0) {
    perror("Could not set socket receive timeout");
    close(sock);
    return -1;
  }

  struct sockaddr_rc sock_address;
  sock_address.rc_family  = AF_BLUETOOTH;
  sock_address.rc_channel = BOSE_CHANNEL;
  if (str2ba(address, &sock_address.rc_bdaddr) != 0) {
    fprintf(stderr, "Invalid bluetooth sock_address: %s\n", address);
    close(sock);
    return -1;
  }
  if (connect(sock, (struct sockaddr *)&sock_address, sizeof(sock_address)) !=
      0) {
    perror("Could not connect to Bluetooth device");
    close(sock);
    return -1;
  }

  int connection = init_connection(sock);
  if (connection) {
    close(sock);
    return -1;
  }

  return sock;
}

int main(int argc, char *argv[]) {
  static const char *        short_opt  = "hidfsban:n:o:c:l:v:p:e:";
  static const struct option long_opt[] = {
      {"help", no_argument, NULL, 'h'},
      {"info", no_argument, NULL, 'i'},
      {"device-status", no_argument, NULL, 'd'},
      {"firmware-version", no_argument, NULL, 'f'},
      {"serial-number", no_argument, NULL, 's'},
      {"battery-level", no_argument, NULL, 'b'},
      {"paired-devices", no_argument, NULL, 'a'},
      {"device-id", no_argument, NULL, 5},
      {"name", required_argument, NULL, 'n'},
      {"auto-off", required_argument, NULL, 'o'},
      {"noise-cancelling", required_argument, NULL, 'c'},
      {"prompt-language", required_argument, NULL, 'l'},
      {"voice-prompts", required_argument, NULL, 'v'},
      {"pairing", required_argument, NULL, 'p'},
      {"self-voice", required_argument, NULL, 'e'},
      {"connect-device", required_argument, NULL, 2},
      {"disconnect-device", required_argument, NULL, 3},
      {"remove-device", required_argument, NULL, 4},
      {"send-packet", required_argument, NULL, 1},
      {0, no_argument, NULL, 0}};

  // Find connection address and verify options
  int opt_index = 0;
  int opt       = getopt_long(argc, argv, short_opt, long_opt, &opt_index);
  switch (opt) {
  case 'h':
    usage();
    return 0;
  case '?':
    usage();
    return 1;
  default:
    break;
  }

  if (argc - 1 != optind) {
    fprintf(stderr, argc <= optind
                        ? "An address argument must be given.\n"
                        : "Only one address argument may be given.\n");
    usage();
    return 1;
  }

  char *address = argv[optind];
  int   status  = 0;
  opt_index     = 0;
  optind        = 1;
  while ((opt = getopt_long(argc, argv, short_opt, long_opt, &opt_index)) > 0) {
    if (status) {
      break;
    }

    switch (opt) {
    case 'i':
      status = do_get_information(address);
      break;
    case 'd':
      status = do_get_device_status(address);
      break;
    case 'f':
      status = do_get_firmware_version(address);
      break;
    case 's':
      status = do_get_serial_number(address);
      break;
    case 'b':
      status = do_get_battery_level(address);
      break;
    case 'a':
      status = do_get_paired_devices(address);
      break;
    case OPTION_DEVICE_ID:
      status = do_get_device_id(address);
      break;
    case 'n':
      status = do_set_name(address, optarg);
      break;
    case 'o':
      status = do_set_auto_off(address, optarg);
      break;
    case 'c':
      status = do_set_noise_cancelling(address, optarg);
      break;
    case 'l':
      status = do_set_prompt_language(address, optarg);
      break;
    case 'v':
      status = do_set_voice_prompts(address, optarg);
      break;
    case 'p':
      status = do_set_pairing(address, optarg);
      break;
    case OPTION_CONNECT_DEVICE:
      status = do_connect_device(address, optarg);
      break;
    case OPTION_DISCONNECT_DEVICE:
      status = do_disconnect_device(address, optarg);
      break;
    case OPTION_REMOVE_DEVICE:
      status = do_remove_device(address, optarg);
      break;
    case 'e':
      status = do_set_self_voice(address, optarg);
      break;
    case OPTION_SEND_PACKET:
      status = do_send_packet(address, optarg);
      break;
    default:
      status = 1;
    }
  }

  if (status < 0) {
    perror("Error trying to change setting");
  }

  return status;
}
