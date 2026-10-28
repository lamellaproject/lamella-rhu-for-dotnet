/* The P/Invoke-facing native surface of the .NET runtime pack at the pin:
 * the declarations of every exported entry point of the five class-library
 * native libraries (libSystem.Native.a, libSystem.Globalization.Native.a, libSystem.IO.Compression.Native.a, libSystem.Net.Security.Native.a, libSystem.Security.Cryptography.Native.OpenSsl.a) that the shim's
 * per-area contract headers do not declare. GENERATED from the runtime's
 * own headers at tag v11.0.0-rc.2.26504.105
 * (commit 820230c386db5b0106c5430b3e5aee92a7826bf5)
 * and the pack's export tables; every name is read off the pack's archives
 * and every signature off the header that declares it, never typed in by
 * hand: regenerate at a re-pin, never edit by hand. The types are mapped by
 * the generator's stated rules: a pointee type the shim does not define is
 * an opaque struct here; an enumeration by value is int32_t (the managed
 * side's marshalling); a scalar alias is its definition at the pin; a
 * function-pointer typedef keeps its signature; a struct passed by value is
 * refused at generation. The layouts below are asserted at the 32-bit ARM
 * EABI the shim is built for.
 *
 * System.Native/pal_autoreleasepool.h sha256 72aa0084ae860d291e795b1ba382f55d34eab7c1ee266c2d3a37c3f8c481a95d
 * System.Native/pal_console.h sha256 5fbac19cf9e7e21717cade7a37b0026bae87ae457992b57cef3cc58db341a7a0
 * System.Native/pal_crossprocessmutex.h sha256 5f5bf872fb65b033a6d7beadc979c448def8accfa032ffffeb593dfd14898848
 * System.Native/pal_datetime.h sha256 ba44203ca2d39c608030e1fd68ac9e1c77a407d8372a8c343d659c13a7935406
 * System.Native/pal_dynamicload.h sha256 da40a3dbc3b0047181cb08d5efa8ab937edd57ef3605e3bfe0b37fa2b2038cec
 * System.Native/pal_environment.h sha256 02d7f51abd87f981e37c0fe5fbd256f6e42956a4c902b2ce26e8adbc21ffe56f
 * System.Native/pal_errno.h sha256 044eecca38fe459aeb1dafba88ed1c3fae82ec0841f25e89dca461ec2ede2009
 * System.Native/pal_getosinfo.h sha256 0382c866dd00f166cc0f44522ed2f7eb2356fa19fa4003f94038afb2992e3a81
 * System.Native/pal_interfaceaddresses.h sha256 034ec3dbaeb61093931c7cfbb686cd130668f41e88edfe500d71ea057b8fa89d
 * System.Native/pal_io.h sha256 e9a61d56707712622798cd431a75d0d853a7d8c719464cc1c5202f9aeb342525
 * System.Native/pal_iossupportversion.h sha256 40f511d713b78942ea38471f24909840ba9dab3b977f082839d2ef5b85cf5188
 * System.Native/pal_log.h sha256 6a0b5d955c4fa6f0df8c136676df030e012c3cc00fb638166dd3b68586c0caad
 * System.Native/pal_maphardwaretype.h sha256 42d71e0a4667d24c5f9eff7e7cf35c9b0d3235d2a4333ebe6d19133351c84024
 * System.Native/pal_memory.h sha256 e6090cb33690e859fc52e8aaaa3e980f768d7cba9d2819dc8a6a807f79ad15de
 * System.Native/pal_mount.h sha256 fa62038696b68b0dbe953bfdf2cb84959684ee9133aba234dcccf569ad85a408
 * System.Native/pal_networkchange.h sha256 3ff72a9ef3b0c2449338fb15cacd8d74d910644686f0d029faf13c10ada03d07
 * System.Native/pal_networking.h sha256 e60b7603b3c9ef53639ca07f0edf27fc1fa279e228e32cfe127581e9ca2a2567
 * System.Native/pal_networkstatistics.h sha256 7c193ef7b3a065f1c4cf94e1fe6e1dd40ed7ea8c5bef8bb76725680d055fea05
 * System.Native/pal_process.h sha256 f683cb314ec0c5685c4dad2a70114d02176b056afcdcbc120b3f0469301d4f78
 * System.Native/pal_random.h sha256 074c42357c92cc8ea509a7c0a7e3a3f144810079610a6b32d0aa35684cc664c8
 * System.Native/pal_runtimeinformation.h sha256 0f707e7670c1cd44ee5577e4247754690a6f1f5d3f442ca78765efb33d4fda22
 * System.Native/pal_searchpath.h sha256 3880ababb3ffa4aaacc63029e32d4a08a7087354ec6d0e048b0ec783363f0c92
 * System.Native/pal_signal.h sha256 3ec0da45c9b230e9b20ae78a079d120361887f32cd8d76cf0770c2df2f1044c5
 * System.Native/pal_string.h sha256 9459d6e04951c6f64db94405f68a70156531ff49b4ecc5e8a776136b62a880a9
 * System.Native/pal_sysctl.h sha256 14611b644187dfe6a2e389414c74fecf74d2c56dd6236644c4f61fab4bb96f4d
 * System.Native/pal_tcpstate.h sha256 5b4bfc3e1b61639727fedae041585e4f5ed4d58e181d46efb3dd48f31dc37e8b
 * System.Native/pal_threading.h sha256 6de494e6728038f26e82edd6c3a6eddb0d32155d6157610f43dd134d57ff8dac
 * System.Native/pal_time.h sha256 291c92de17e67a02c2c687d28cd809153a80d0463ad10e41c54e12b05456a51c
 * System.Native/pal_uid.h sha256 74e0c86f705264c778f1b26101784f10e0f4b918fa7af6a977581da4bfb647a9
 * System.Globalization.Native/pal_calendarData.h sha256 5189a4f9f3e6c07661b044321456624cfe5f7cfb2b1ac58dbca5a4d4757fdcb3
 * System.Globalization.Native/pal_casing.h sha256 d5effbd6b0564bf9e5cc3a43093f8ecff320b3ce689e3e54312c37a9e49c8c39
 * System.Globalization.Native/pal_collation.h sha256 b83623bc681063e06d4f684767d5862955ae8f2040fa57dfba603cd99258e46f
 * System.Globalization.Native/pal_errors.h sha256 91c200e05772fbc4eda2ca89590e439b2ecd6413ec99fe1b7ba3dea0e53a46ee
 * System.Globalization.Native/pal_errors_internal.h sha256 30ac2dd574a0d3262a0c34ac1d39914748c28674d869c5b3dc4813f5c92df2e5
 * System.Globalization.Native/pal_icushim.h sha256 261106537736be92bd1688a0b9cf9cdc84ffb27dbaa4819a8cb76861700543c1
 * System.Globalization.Native/pal_icushim_internal.h sha256 5aac4ec1c095a3653331171b034026f1d31a5b0848f7321939279e30839514eb
 * System.Globalization.Native/pal_icushim_internal_android.h sha256 b6554227d2413fd2769adb2a6e27b5675ca4fc014fefcd29ef89b637800d9760
 * System.Globalization.Native/pal_idna.h sha256 2e9d7df64f360ffe395cc2f8391cc1390ea7ee27698c1338bc68f0565f1ec27a
 * System.Globalization.Native/pal_locale.h sha256 03a6da4afbf4767d9de7b6cc7f7d7015f6ce3f0000591eb7894eb205c0f70450
 * System.Globalization.Native/pal_localeNumberData.h sha256 b6eb22dbe6edc5e3fac71239373440a6327ce2e2c629df5dc2f788a52325ec32
 * System.Globalization.Native/pal_localeStringData.h sha256 c6f1616b7fd09ff785f8ec61eb8e9a70fd5b9c465360247ab2ed382c5d0d9b95
 * System.Globalization.Native/pal_locale_internal.h sha256 519a2b6cf83b151a149cae10569f02cbd78530385c1ae0249405c95d0ca2c82f
 * System.Globalization.Native/pal_normalization.h sha256 5383239a01634c5b5f981eb4d58a26cda797d1b00ff23cb3f912bc3eaed1d788
 * System.Globalization.Native/pal_timeZoneInfo.h sha256 12810bf65c337eb2b323ac6aadd989d7e7e727cee1a5f0caa93932083827f3cf
 * System.IO.Compression.Native/pal_zlib.h sha256 3a61ca2817b629cdfee2178b65b7c0fb4e32ca0590078f2013c1e7d851191464
 * System.IO.Compression.Native/zlib_allocator.h sha256 c51e87a6df4c966f4328a5686b89c5fc06113d4590fd6902ef0e0edf821ef0ab
 * System.Net.Security.Native/pal_gssapi.h sha256 3664cee75c34e82e144b764b23537ce2c361b3f24fb27f0f940adcf004956ef5
 * System.Security.Cryptography.Native/apibridge_30.h sha256 c9adf3ee4c51398f2e197e6970fc2be04e787e623f1a2a092a94194d71351dec
 * System.Security.Cryptography.Native/apibridge_30_rev.h sha256 c3040d8b78bce6a061ebbd6dead1110e0945def755e1f7b36ca2dbaa93e23c1d
 * System.Security.Cryptography.Native/memory_debug.h sha256 63ec7bf9ed32bd098a41d069fce3d3c07bf6976e997c48d78c1e3a2b11594fc5
 * System.Security.Cryptography.Native/openssl.h sha256 940cb7c13d6ff05f8a621c31251ed997c88a7b4e3e3455d87595289686fe69a9
 * System.Security.Cryptography.Native/opensslshim.h sha256 2141d97b692315138104c7a6ddb7c580b34f30a4e87584877b194a43f729c5bc
 * System.Security.Cryptography.Native/osslcompat_30.h sha256 9d7bef6f88e72b2c1eeb6dae32b51b98c76fda0cd0ddd4b46b33a388e035ba98
 * System.Security.Cryptography.Native/osslcompat_40.h sha256 4d9bddd5076bc0c4de4cdec957029fb2a1da38fc2bb7ca7451ecb4b38f7ec117
 * System.Security.Cryptography.Native/pal_asn1.h sha256 8482463552cc83306dbba5291d19fb98bd6417835e3b7bb318168bdd23fe27f2
 * System.Security.Cryptography.Native/pal_bignum.h sha256 4d49e42681b98fff9b1a25d595ee5de4d67f74560d8eb13bfbbeef7b8c765a56
 * System.Security.Cryptography.Native/pal_bio.h sha256 fef28e01ed6f31a02bf5ea91d72d62608f7f69c70489996563a0af0bf89c8e90
 * System.Security.Cryptography.Native/pal_crypto_types.h sha256 be43c32a01251b004386507365f086c5bbaa8fb9e0c6cee1dc7ad364f1faafef
 * System.Security.Cryptography.Native/pal_dsa.h sha256 e935fe4a59b3b5989403c0a0a22342b33ba0b0f9ded2af0b6bdd14b444bd50c6
 * System.Security.Cryptography.Native/pal_ecc_import_export.h sha256 0e7ad4ae6507dd6a4712ab3cfcd5afe7f403013be3fa7b18a59a6186315c6841
 * System.Security.Cryptography.Native/pal_eckey.h sha256 bd03ddb685d16551f8995325bba7d047fe7573c12e02bb1ebdcdb39e37af5e39
 * System.Security.Cryptography.Native/pal_err.h sha256 497f23c209de7bf782f6058ee50473c68a5c4e7f5fba0181086c28bb9b18f191
 * System.Security.Cryptography.Native/pal_evp.h sha256 53a6c5c1ddf936165bd5708eb4029c2d64c3cba3aea0f98fabfce62346fbe1c5
 * System.Security.Cryptography.Native/pal_evp_cipher.h sha256 ce1e8999459169dfdfe09fe0ca3ce5d62e365eaa663dd4ca7ef158e6837b783a
 * System.Security.Cryptography.Native/pal_evp_kdf.h sha256 4b3f165b522f1dc1c009c3249e114ed519c6dfc676b46056f9246fb92006f08c
 * System.Security.Cryptography.Native/pal_evp_kem.h sha256 797d17229ea5109b3ba2a6e5394b38554ca836e5be0125bab3f769efd639a682
 * System.Security.Cryptography.Native/pal_evp_mac.h sha256 686a09885922a25e78ca35bb83e13d524088bf6d4c40017b1c43440c2ceddbb3
 * System.Security.Cryptography.Native/pal_evp_pkey.h sha256 8d40f9f21e5f0821911295d6085ebd53d1fba5d09e6728690461180071598bb9
 * System.Security.Cryptography.Native/pal_evp_pkey_dsa.h sha256 0f6b42b9dab9ae05666d6289149ff3a9dbcf3df902950e807cb5ac8579447051
 * System.Security.Cryptography.Native/pal_evp_pkey_ecdh.h sha256 9722620c1dcdbc0a8576ead29314de262219ac19e7b94820ddc73e1933cb2ed8
 * System.Security.Cryptography.Native/pal_evp_pkey_ecdsa.h sha256 afb68ea0fea3b509240d642a3b808c2e851b16d45171e6c8b7ac8a73d4f31e04
 * System.Security.Cryptography.Native/pal_evp_pkey_eckey.h sha256 e21c2268409fa9db55aeab1384b4039f489654ac7690265ad96dd48bd1c3e0d3
 * System.Security.Cryptography.Native/pal_evp_pkey_ml_dsa.h sha256 a57f0d1c340f0ca97d50265c96823a976e30d97bfd3286bace8d94ca5e33a3b5
 * System.Security.Cryptography.Native/pal_evp_pkey_raw_signverify.h sha256 05453999574812efa0b6cfe5142d53d9df0ee838c337e3725754b040e8da0393
 * System.Security.Cryptography.Native/pal_evp_pkey_rsa.h sha256 019fe4303f9a6272696d66fa955d48ba1de75ebee714cceef8cc0be3a2784b7b
 * System.Security.Cryptography.Native/pal_evp_pkey_slh_dsa.h sha256 b23864d6e2bcf3177859ebe17e40d3f3c38f3d5d80b6d5413a8d99c473c86060
 * System.Security.Cryptography.Native/pal_evp_pkey_x25519.h sha256 5738f411f19f7e447ac31bf06b0fa47433402d639dbc5be01e0ebfe5bafb96b0
 * System.Security.Cryptography.Native/pal_hmac.h sha256 cbd9e30b5a49bc9b7827a2e0f87ae75b0ea284972757c7935b1334fc0545b0f6
 * System.Security.Cryptography.Native/pal_ocsp.h sha256 8d06d729665cf839d94bf4d6cebece2a081c1667ed67bdfa6efb12c9b32ec855
 * System.Security.Cryptography.Native/pal_pkcs7.h sha256 792f146801fd6aa6e2a1db97886ea3a7e79f58fd543df72a4768aaca9941f459
 * System.Security.Cryptography.Native/pal_ssl.h sha256 58f3bf94c29399e5c6cfe2e2b2436ff0d81cd6783b4af8ededa8c74dc1e0357c
 * System.Security.Cryptography.Native/pal_x509.h sha256 de7821b32b21718c9f8762c13a5ec56a38c229a21cb68f7449e63cf69de079ee
 * System.Security.Cryptography.Native/pal_x509_name.h sha256 8d8cc044b146d2d7355da101cd0be774ff9a47bcbd50d4fa933f673d237eed02
 * System.Security.Cryptography.Native/pal_x509_root.h sha256 038dd3962aa623218ca98ea92d390788b2f9688ce0f9d505e9fb5cc3af3d23f0
 * System.Security.Cryptography.Native/pal_x509ext.h sha256 6bc0e5eeadd0f9b4d40fd8ef8be5d54cf88a4105c9d5abd2f25af6a3bf4e9125
 */
#ifndef RHU_PAL_SURFACE_NET11_RC2_H
#define RHU_PAL_SURFACE_NET11_RC2_H

#include <stddef.h>
#include <stdint.h>
#include "rhu_pal_uid_net11_rc2.h"

#ifndef RHU_PAL_STATIC_ASSERT
#ifdef __cplusplus
#define RHU_PAL_STATIC_ASSERT(cond, msg) static_assert(cond, msg)
#else
#define RHU_PAL_STATIC_ASSERT(cond, msg) _Static_assert(cond, msg)
#endif
#endif

/* Opaque types: named by the PAL's (and its libraries') spellings, never defined here. */
typedef struct ASN1_BIT_STRING ASN1_BIT_STRING;
typedef struct ASN1_INTEGER ASN1_INTEGER;
typedef struct ASN1_OBJECT ASN1_OBJECT;
typedef struct ASN1_OCTET_STRING ASN1_OCTET_STRING;
typedef struct ASN1_STRING ASN1_STRING;
typedef struct ASN1_TIME ASN1_TIME;
typedef struct AreaInfo AreaInfo;
typedef struct BIGNUM BIGNUM;
typedef struct BIO BIO;
typedef struct DN_MAC_CTX DN_MAC_CTX;
typedef struct DSA DSA;
typedef struct EC_KEY EC_KEY;
typedef struct EVP_CIPHER EVP_CIPHER;
typedef struct EVP_CIPHER_CTX EVP_CIPHER_CTX;
typedef struct EVP_KDF EVP_KDF;
typedef struct EVP_MAC EVP_MAC;
typedef struct EVP_MAC_CTX EVP_MAC_CTX;
typedef struct EVP_MD EVP_MD;
typedef struct EVP_MD_CTX EVP_MD_CTX;
typedef struct EVP_PKEY EVP_PKEY;
typedef struct GssCredId GssCredId;
typedef struct GssCtxId GssCtxId;
typedef struct GssName GssName;
typedef struct HostEntry HostEntry;
typedef struct IPEndPointInfo IPEndPointInfo;
typedef struct IPPacketInformation IPPacketInformation;
typedef struct IPv4GlobalStatistics IPv4GlobalStatistics;
typedef struct IPv4MulticastOption IPv4MulticastOption;
typedef struct IPv6MulticastOption IPv6MulticastOption;
typedef struct Icmpv4GlobalStatistics Icmpv4GlobalStatistics;
typedef struct Icmpv6GlobalStatistics Icmpv6GlobalStatistics;
typedef struct IpAddressInfo IpAddressInfo;
typedef struct LingerOption LingerOption;
typedef struct LinkLayerAddressInfo LinkLayerAddressInfo;
typedef struct LowLevelCrossProcessMutex LowLevelCrossProcessMutex;
typedef struct MessageHeader MessageHeader;
typedef struct NativeIPInterfaceStatistics NativeIPInterfaceStatistics;
typedef struct NativeTcpConnectionInformation NativeTcpConnectionInformation;
typedef struct NetworkInterfaceInfo NetworkInterfaceInfo;
typedef struct OCSP_REQUEST OCSP_REQUEST;
typedef struct OCSP_RESPONSE OCSP_RESPONSE;
typedef struct PAL_GssBuffer PAL_GssBuffer;
typedef struct PAL_ZStream PAL_ZStream;
typedef struct PKCS7 PKCS7;
typedef struct ProcessInfo ProcessInfo;
typedef struct RSA RSA;
typedef struct SSL SSL;
typedef struct SSL_CTX SSL_CTX;
typedef struct SSL_METHOD SSL_METHOD;
typedef struct SSL_SESSION SSL_SESSION;
typedef struct SocketEvent SocketEvent;
typedef struct SortHandle SortHandle;
typedef struct TcpGlobalStatistics TcpGlobalStatistics;
typedef struct ThreadInfo ThreadInfo;
typedef struct UdpGlobalStatistics UdpGlobalStatistics;
typedef struct X509 X509;
typedef struct X509NameStack X509NameStack;
typedef struct X509Stack X509Stack;
typedef struct X509_CRL X509_CRL;
typedef struct X509_EXTENSION X509_EXTENSION;
typedef struct X509_NAME X509_NAME;
typedef struct X509_STORE X509_STORE;
typedef struct X509_STORE_CTX X509_STORE_CTX;
typedef struct X509_stack X509_stack;

/* rhu_pal_rlimit: the PAL's RLimit (pal_process.h), field for field; offsets and size at the 32-bit ARM
 * EABI asserted below. */
typedef struct rhu_pal_rlimit {
    uint64_t CurrentLimit;
    uint64_t MaximumLimit;
} rhu_pal_rlimit;
RHU_PAL_STATIC_ASSERT(sizeof(rhu_pal_rlimit) == 16, "rhu_pal_rlimit size");
RHU_PAL_STATIC_ASSERT(offsetof(rhu_pal_rlimit, CurrentLimit) == 0, "rhu_pal_rlimit.CurrentLimit offset");
RHU_PAL_STATIC_ASSERT(offsetof(rhu_pal_rlimit, MaximumLimit) == 8, "rhu_pal_rlimit.MaximumLimit offset");

/* rhu_pal_mountpointinformation: the PAL's MountPointInformation (pal_mount.h), field for field; offsets and size at the 32-bit ARM
 * EABI asserted below. */
typedef struct rhu_pal_mountpointinformation {
    uint64_t AvailableFreeSpace;
    uint64_t TotalFreeSpace;
    uint64_t TotalSize;
} rhu_pal_mountpointinformation;
RHU_PAL_STATIC_ASSERT(sizeof(rhu_pal_mountpointinformation) == 24, "rhu_pal_mountpointinformation size");
RHU_PAL_STATIC_ASSERT(offsetof(rhu_pal_mountpointinformation, AvailableFreeSpace) == 0, "rhu_pal_mountpointinformation.AvailableFreeSpace offset");
RHU_PAL_STATIC_ASSERT(offsetof(rhu_pal_mountpointinformation, TotalFreeSpace) == 8, "rhu_pal_mountpointinformation.TotalFreeSpace offset");
RHU_PAL_STATIC_ASSERT(offsetof(rhu_pal_mountpointinformation, TotalSize) == 16, "rhu_pal_mountpointinformation.TotalSize offset");

/* RLimitResources (pal_process.h at the pin) */
#define RHU_PAL_RLIMIT_CPU 0
#define RHU_PAL_RLIMIT_FSIZE 1
#define RHU_PAL_RLIMIT_DATA 2
#define RHU_PAL_RLIMIT_STACK 3
#define RHU_PAL_RLIMIT_CORE 4
#define RHU_PAL_RLIMIT_AS 5
#define RHU_PAL_RLIMIT_RSS 6
#define RHU_PAL_RLIMIT_MEMLOCK 7
#define RHU_PAL_RLIMIT_NPROC 8
#define RHU_PAL_RLIMIT_NOFILE 9

/* SysLogPriority (pal_process.h at the pin) */
#define RHU_PAL_LOG_EMERG 0
#define RHU_PAL_LOG_ALERT 1
#define RHU_PAL_LOG_CRIT 2
#define RHU_PAL_LOG_ERR 3
#define RHU_PAL_LOG_WARNING 4
#define RHU_PAL_LOG_NOTICE 5
#define RHU_PAL_LOG_INFO 6
#define RHU_PAL_LOG_DEBUG 7

/* PathConfName (pal_process.h at the pin) */
#define RHU_PAL_PC_LINK_MAX 1
#define RHU_PAL_PC_MAX_CANON 2
#define RHU_PAL_PC_MAX_INPUT 3
#define RHU_PAL_PC_NAME_MAX 4
#define RHU_PAL_PC_PATH_MAX 5
#define RHU_PAL_PC_PIPE_BUF 6
#define RHU_PAL_PC_CHOWN_RESTRICTED 7
#define RHU_PAL_PC_NO_TRUNC 8
#define RHU_PAL_PC_VDISABLE 9

/* PriorityWhich (pal_process.h at the pin) */
#define RHU_PAL_PRIO_PROCESS 0
#define RHU_PAL_PRIO_PGRP 1
#define RHU_PAL_PRIO_USER 2

/* GetAddrInfoErrorFlags (pal_networking.h at the pin) */
#define RHU_PAL_GetAddrInfoErrorFlags_EAI_SUCCESS 0
#define RHU_PAL_GetAddrInfoErrorFlags_EAI_AGAIN 1
#define RHU_PAL_GetAddrInfoErrorFlags_EAI_BADFLAGS 2
#define RHU_PAL_GetAddrInfoErrorFlags_EAI_FAIL 3
#define RHU_PAL_GetAddrInfoErrorFlags_EAI_FAMILY 4
#define RHU_PAL_GetAddrInfoErrorFlags_EAI_NONAME 5
#define RHU_PAL_GetAddrInfoErrorFlags_EAI_BADARG 6
#define RHU_PAL_GetAddrInfoErrorFlags_EAI_NOMORE 7
#define RHU_PAL_GetAddrInfoErrorFlags_EAI_MEMORY 8
#define RHU_PAL_GetAddrInfoErrorFlags_EAI_SYSTEM 9

/* ResultCode (pal_errors.h at the pin) */
#define RHU_PAL_Success 0
#define RHU_PAL_UnknownError 1
#define RHU_PAL_InsufficientBuffer 2
#define RHU_PAL_OutOfMemory 3
#define RHU_PAL_InvalidCodePoint 4

/* Callback signatures at the pin. */
typedef void (*EnumCalendarInfoCallback)(const uint16_t* p0, const void* p1);
typedef void (*GatewayAddressFound)(void* context, IpAddressInfo* addressInfo);
typedef void (*IPv4AddressFound)(void* context, const char* interfaceName, IpAddressInfo* addressInfo);
typedef void (*IPv6AddressFound)(void* context, const char* interfaceName, IpAddressInfo* info, uint32_t* scopeId);
typedef void (*LinkLayerAddressFound)(void* context, const char* interfaceName, LinkLayerAddressInfo* llAddress);
typedef void (*MountPointFound)(void* context, const char* name);
typedef void (*NetworkChangeEvent)(intptr_t sock, int32_t notificationKind);
typedef int (*SslCtxCertVerifyCallback)(X509_STORE_CTX* store, void* arg);
typedef int32_t (*SslCtxNewSessionCallback)(SSL* ssl, SSL_SESSION* session);
typedef void (*SslCtxRemoveSessionCallback)(SSL_CTX* ctx, SSL_SESSION* session);
typedef int32_t (*SslCtxSetAlpnCallback)(SSL* ssl, const uint8_t** out, uint8_t* outlen, const uint8_t* in, uint32_t inlen, void* arg);
typedef void (*SslCtxSetKeylogCallback)(const SSL* ssl, const char* line);
typedef int32_t (*X509StoreVerifyCallback)(int32_t p0, X509_STORE_CTX* p1);

#ifdef __cplusplus
extern "C" {
#endif

/* libSystem.Native.a: SystemNative_ */
/* pal_autoreleasepool.h */
void* SystemNative_CreateAutoreleasePool(void);
void SystemNative_DrainAutoreleasePool(void* pool);
/* pal_crossprocessmutex.h */
int32_t SystemNative_LowLevelCrossProcessMutex_Acquire(LowLevelCrossProcessMutex* mutex, int32_t timeoutMilliseconds);
int32_t SystemNative_LowLevelCrossProcessMutex_Destroy(LowLevelCrossProcessMutex* mutex);
void SystemNative_LowLevelCrossProcessMutex_GetOwnerProcessAndThreadId(LowLevelCrossProcessMutex* mutex, uint32_t* pOwnerProcessId, uint32_t* pOwnerThreadId);
int32_t SystemNative_LowLevelCrossProcessMutex_Init(LowLevelCrossProcessMutex* mutex);
uint8_t SystemNative_LowLevelCrossProcessMutex_IsAbandoned(LowLevelCrossProcessMutex* mutex);
int32_t SystemNative_LowLevelCrossProcessMutex_Release(LowLevelCrossProcessMutex* mutex);
void SystemNative_LowLevelCrossProcessMutex_SetAbandoned(LowLevelCrossProcessMutex* mutex, uint8_t isAbandoned);
void SystemNative_LowLevelCrossProcessMutex_SetOwnerProcessAndThreadId(LowLevelCrossProcessMutex* mutex, uint32_t ownerProcessId, uint32_t ownerThreadId);
int32_t SystemNative_LowLevelCrossProcessMutex_Size(void);
/* pal_datetime.h */
char* SystemNative_GetDefaultTimeZone(void);
const char* SystemNative_GetTimeZoneData(const char* name, int* length);
/* pal_dynamicload.h */
void SystemNative_FreeLibrary(void* handle);
void* SystemNative_GetDefaultSearchOrderPseudoHandle(void);
void* SystemNative_GetLoadLibraryError(void);
void* SystemNative_GetProcAddress(void* handle, const char* symbol);
void* SystemNative_LoadLibrary(const char* filename);
/* pal_environment.h */
void SystemNative_FreeEnviron(char** environ);
char** SystemNative_GetEnviron(void);
/* pal_getosinfo.h */
int32_t SystemNative_GetNextAreaInfo(int32_t team, intptr_t* cookie, AreaInfo* areaInfo);
/* pal_interfaceaddresses.h */
int32_t SystemNative_EnumerateGatewayAddressesForInterface(void* context, uint32_t interfaceIndex, GatewayAddressFound onGatewayFound);
int32_t SystemNative_EnumerateInterfaceAddresses(void* context, IPv4AddressFound onIpv4Found, IPv6AddressFound onIpv6Found, LinkLayerAddressFound onLinkLayerFound);
int32_t SystemNative_GetNetworkInterfaces(int32_t* interfaceCount, NetworkInterfaceInfo** interfaces, int32_t* addressCount, IpAddressInfo** addressList);
/* pal_io.h */
int32_t SystemNative_CanGetHiddenFlag(void);
int32_t SystemNative_ChDir(const char* path);
int32_t SystemNative_ChMod(const char* path, int32_t mode);
int32_t SystemNative_CopyFile(intptr_t sourceFd, intptr_t destinationFd, int64_t sourceLength);
int32_t SystemNative_FAllocate(intptr_t fd, int64_t offset, int64_t length);
int32_t SystemNative_FChMod(intptr_t fd, int32_t mode);
int32_t SystemNative_FChflags(intptr_t fd, uint32_t flags);
int32_t SystemNative_FcntlCanGetSetPipeSz(void);
int32_t SystemNative_FcntlGetFD(intptr_t fd);
int32_t SystemNative_FcntlGetPipeSz(intptr_t fd);
int32_t SystemNative_FcntlSetFD(intptr_t fd, int32_t flags);
int32_t SystemNative_FcntlSetIsNonBlocking(intptr_t fd, int32_t isNonBlocking);
int32_t SystemNative_FcntlSetPipeSz(intptr_t fd, int32_t size);
uint32_t SystemNative_FileSystemSupportsLocking(intptr_t fd, int32_t lockOperation, int32_t accessWrite);
void SystemNative_GetDeviceIdentifiers(uint64_t dev, uint32_t* majorNumber, uint32_t* minorNumber);
int32_t SystemNative_GetPeerID(intptr_t socket, uint32_t* euid);
int32_t SystemNative_INotifyAddWatch(intptr_t fd, const char* pathName, uint32_t mask);
intptr_t SystemNative_INotifyInit(void);
int32_t SystemNative_INotifyRemoveWatch(intptr_t fd, int32_t wd);
int32_t SystemNative_IsAtomicNonInheritablePipeCreationSupported(void);
int32_t SystemNative_LChflags(const char* path, uint32_t flags);
int32_t SystemNative_LChflagsCanSetHiddenFlag(void);
int32_t SystemNative_Link(const char* source, const char* linkTarget);
int32_t SystemNative_LockFileRegion(intptr_t fd, int64_t offset, int64_t length, int16_t lockType);
int32_t SystemNative_MkDirAt(intptr_t dirfd, const char* path, int32_t mode);
int32_t SystemNative_MkFifo(const char* pathName, uint32_t mode);
int32_t SystemNative_MkNod(const char* pathName, uint32_t mode, uint32_t major, uint32_t minor);
char* SystemNative_MkdTemp(char* pathTemplate);
intptr_t SystemNative_MksTemps(char* pathTemplate, int32_t suffixLength);
intptr_t SystemNative_OpenAt(intptr_t dirfd, const char* path, int32_t flags, int32_t mode);
int32_t SystemNative_PosixFAdvise(intptr_t fd, int64_t offset, int64_t length, int32_t advice);
int32_t SystemNative_ReadLink(const char* path, char* buffer, int32_t bufferSize);
int32_t SystemNative_ReadProcessInfo(int32_t pid, ProcessInfo* processInfo, uint8_t* argBuf, int32_t argBufSize);
int32_t SystemNative_ReadThreadInfo(int32_t pid, int32_t tid, ThreadInfo* threadInfo);
char* SystemNative_RealPath(const char* path);
int32_t SystemNative_SymLink(const char* target, const char* linkPath);
void SystemNative_Sync(void);
int32_t SystemNative_UnlinkAt(intptr_t dirfd, const char* path, int32_t flags);
int32_t SystemNative_WriteToNonblocking(intptr_t fd, const void* buffer, int32_t bufferSize);
/* pal_iossupportversion.h */
const char* SystemNative_iOSSupportVersion(void);
/* pal_mount.h */
int32_t SystemNative_GetAllMountPoints(MountPointFound onFound, void* context);
int32_t SystemNative_GetFileSystemTypeNameForMountPoint(const char* name, char* formatNameBuffer, int32_t bufferLength, int64_t* formatType);
int32_t SystemNative_GetSpaceInfoForMountPoint(const char* name, rhu_pal_mountpointinformation* mpi);
/* pal_networkchange.h */
int32_t SystemNative_CreateNetworkChangeListenerSocket(intptr_t* retSocket);
int32_t SystemNative_ReadEvents(intptr_t sock, NetworkChangeEvent onNetworkChange);
/* pal_networking.h */
int32_t SystemNative_Accept(intptr_t socket, uint8_t* socketAddress, int32_t* socketAddressLen, intptr_t* acceptedSocket);
int32_t SystemNative_Bind(intptr_t socket, int32_t protocolType, uint8_t* socketAddress, int32_t socketAddressLen);
int32_t SystemNative_CloseSocketEventPort(intptr_t port);
int32_t SystemNative_Connect(intptr_t socket, uint8_t* socketAddress, int32_t socketAddressLen);
int32_t SystemNative_Connectx(intptr_t socket, uint8_t* socketAddress, int32_t socketAddressLen, uint8_t* data, int32_t dataLen, int32_t tfo, int* sent);
int32_t SystemNative_CreateSocketEventBuffer(int32_t count, SocketEvent** buffer);
int32_t SystemNative_CreateSocketEventPort(intptr_t* port);
int32_t SystemNative_Disconnect(intptr_t socket);
void SystemNative_FreeHostEntry(HostEntry* entry);
int32_t SystemNative_FreeSocketEventBuffer(SocketEvent* buffer);
int32_t SystemNative_GetAddressFamily(const uint8_t* socketAddress, int32_t socketAddressLen, int32_t* addressFamily);
int32_t SystemNative_GetAtOutOfBandMark(intptr_t socket, int32_t* available);
int32_t SystemNative_GetBytesAvailable(intptr_t socket, int32_t* available);
int32_t SystemNative_GetControlMessageBufferSize(int32_t isIPv4, int32_t isIPv6);
int32_t SystemNative_GetDomainName(uint8_t* name, int32_t nameLength);
void SystemNative_GetDomainSocketSizes(int32_t* pathOffset, int32_t* pathSize, int32_t* addressSize);
int32_t SystemNative_GetHostEntryForName(const uint8_t* address, int32_t addressFamily, HostEntry* entry);
int32_t SystemNative_GetHostName(uint8_t* name, int32_t nameLength);
int32_t SystemNative_GetIPv4Address(const uint8_t* socketAddress, int32_t socketAddressLen, uint32_t* address);
int32_t SystemNative_GetIPv4MulticastOption(intptr_t socket, int32_t multicastOption, IPv4MulticastOption* option);
int32_t SystemNative_GetIPv6Address(const uint8_t* socketAddress, int32_t socketAddressLen, uint8_t* address, int32_t addressLen, uint32_t* scopeId);
int32_t SystemNative_GetIPv6MulticastOption(intptr_t socket, int32_t multicastOption, IPv6MulticastOption* option);
int32_t SystemNative_GetLingerOption(intptr_t socket, LingerOption* option);
int32_t SystemNative_GetMaximumAddressSize(void);
int32_t SystemNative_GetNameInfo(const uint8_t* address, int32_t addressLength, int8_t isIPv6, uint8_t* host, int32_t hostLength, uint8_t* service, int32_t serviceLength, int32_t flags);
int32_t SystemNative_GetPeerName(intptr_t socket, uint8_t* socketAddress, int32_t* socketAddressLen);
int32_t SystemNative_GetPort(const uint8_t* socketAddress, int32_t socketAddressLen, uint16_t* port);
int32_t SystemNative_GetRawSockOpt(intptr_t socket, int32_t socketOptionLevel, int32_t socketOptionName, uint8_t* optionValue, int32_t* optionLen);
int32_t SystemNative_GetSockName(intptr_t socket, uint8_t* socketAddress, int32_t* socketAddressLen);
int32_t SystemNative_GetSockOpt(intptr_t socket, int32_t socketOptionLevel, int32_t socketOptionName, uint8_t* optionValue, int32_t* optionLen);
int32_t SystemNative_GetSocketAddressSizes(int32_t* ipv4SocketAddressSize, int32_t* ipv6SocketAddressSize, int32_t* udsSocketAddressSize, int32_t* maxSocketAddressSize);
int32_t SystemNative_GetSocketErrorOption(intptr_t socket, int32_t* error);
int32_t SystemNative_GetSocketType(intptr_t socket, int32_t* addressFamily, int32_t* socketType, int32_t* protocolType, int32_t* isListening);
int32_t SystemNative_GetWasiSocketDescriptor(intptr_t socket, void** entry, int32_t* socketType);
uint32_t SystemNative_InterfaceNameToIndex(char* interfaceName);
int32_t SystemNative_Listen(intptr_t socket, int32_t backlog);
int32_t SystemNative_PlatformSupportsDualModeIPv4PacketInfo(void);
int32_t SystemNative_Receive(intptr_t socket, void* buffer, int32_t bufferLen, int32_t flags, int32_t* received);
int32_t SystemNative_ReceiveMessage(intptr_t socket, MessageHeader* messageHeader, int32_t flags, int64_t* received);
int32_t SystemNative_ReceiveSocketError(intptr_t socket, MessageHeader* messageHeader);
int32_t SystemNative_Select(int* readFds, int readFdsCount, int* writeFds, int writeFdsCount, int* errorFds, int errorFdsCount, int32_t microseconds, int32_t maxFd, int* triggered);
int32_t SystemNative_Send(intptr_t socket, void* buffer, int32_t bufferLen, int32_t flags, int32_t* sent);
int32_t SystemNative_SendFile(intptr_t out_fd, intptr_t in_fd, int64_t offset, int64_t count, int64_t* sent);
int32_t SystemNative_SendMessage(intptr_t socket, MessageHeader* messageHeader, int32_t flags, int64_t* sent);
int32_t SystemNative_SetAddressFamily(uint8_t* socketAddress, int32_t socketAddressLen, int32_t addressFamily);
int32_t SystemNative_SetIPv4Address(uint8_t* socketAddress, int32_t socketAddressLen, uint32_t address);
int32_t SystemNative_SetIPv4MulticastOption(intptr_t socket, int32_t multicastOption, IPv4MulticastOption* option);
int32_t SystemNative_SetIPv6Address(uint8_t* socketAddress, int32_t socketAddressLen, uint8_t* address, int32_t addressLen, uint32_t scopeId);
int32_t SystemNative_SetIPv6MulticastOption(intptr_t socket, int32_t multicastOption, IPv6MulticastOption* option);
int32_t SystemNative_SetLingerOption(intptr_t socket, LingerOption* option);
int32_t SystemNative_SetPort(uint8_t* socketAddress, int32_t socketAddressLen, uint16_t port);
int32_t SystemNative_SetRawSockOpt(intptr_t socket, int32_t socketOptionLevel, int32_t socketOptionName, uint8_t* optionValue, int32_t optionLen);
int32_t SystemNative_SetReceiveTimeout(intptr_t socket, int32_t millisecondsTimeout);
int32_t SystemNative_SetSendTimeout(intptr_t socket, int32_t millisecondsTimeout);
int32_t SystemNative_SetSockOpt(intptr_t socket, int32_t socketOptionLevel, int32_t socketOptionName, uint8_t* optionValue, int32_t optionLen);
int32_t SystemNative_Shutdown(intptr_t socket, int32_t socketShutdown);
int32_t SystemNative_Socket(int32_t addressFamily, int32_t socketType, int32_t protocolType, intptr_t* createdSocket);
int32_t SystemNative_TryChangeSocketEventRegistration(intptr_t port, intptr_t socket, int32_t currentEvents, int32_t newEvents, uintptr_t data);
int32_t SystemNative_TryGetIPPacketInformation(MessageHeader* messageHeader, int32_t isIPv4, IPPacketInformation* packetInfo);
int32_t SystemNative_WaitForSocketEvents(intptr_t port, SocketEvent* buffer, int32_t* count);
int32_t SystemNative_WasiSubscribeSocketPollable(int32_t kind, int32_t handle);
/* pal_networkstatistics.h */
int32_t SystemNative_GetActiveTcpConnectionInfos(NativeTcpConnectionInformation* infos, int32_t* infoCount);
int32_t SystemNative_GetActiveUdpListeners(IPEndPointInfo* infos, int32_t* infoCount);
int32_t SystemNative_GetEstimatedTcpConnectionCount(void);
int32_t SystemNative_GetEstimatedUdpListenerCount(void);
int32_t SystemNative_GetIPv4GlobalStatistics(IPv4GlobalStatistics* retStats);
int32_t SystemNative_GetIcmpv4GlobalStatistics(Icmpv4GlobalStatistics* retStats);
int32_t SystemNative_GetIcmpv6GlobalStatistics(Icmpv6GlobalStatistics* retStats);
int32_t SystemNative_GetNativeIPInterfaceStatistics(char* interfaceName, NativeIPInterfaceStatistics* retStats);
int32_t SystemNative_GetNumRoutes(void);
int32_t SystemNative_GetTcpGlobalStatistics(TcpGlobalStatistics* retStats);
int32_t SystemNative_GetUdpGlobalStatistics(UdpGlobalStatistics* retStats);
/* pal_process.h */
int32_t SystemNative_ForkAndExecProcess(const char* filename, char* const argv[], char* const envp[], const char* cwd, int32_t setCredentials, uint32_t userId, uint32_t groupId, uint32_t* groups, int32_t groupsLength, int32_t* childPid, int32_t stdinFd, int32_t stdoutFd, int32_t stderrFd, int32_t* inheritedFds, int32_t inheritedFdCount, int32_t startDetached, int32_t killOnParentExit, int32_t startSuspended);
int32_t SystemNative_GetPid(void);
int32_t SystemNative_GetPriority(int32_t which, int32_t who);
char* SystemNative_GetProcessPath(void);
int32_t SystemNative_GetRLimit(int32_t resourceType, rhu_pal_rlimit* limits);
int32_t SystemNative_GetSid(int32_t pid);
int32_t SystemNative_Kill(int32_t pid, int32_t signal);
int64_t SystemNative_PathConf(const char* path, int32_t name);
int32_t SystemNative_SchedGetAffinity(int32_t pid, intptr_t* mask);
int32_t SystemNative_SchedSetAffinity(int32_t pid, intptr_t* mask);
int32_t SystemNative_SetPriority(int32_t which, int32_t who, int32_t nice);
int32_t SystemNative_SetRLimit(int32_t resourceType, const rhu_pal_rlimit* limits);
void SystemNative_SysLog(int32_t priority, const char* message, const char* arg1);
int32_t SystemNative_WaitIdAnyExitedNoHangNoWait(void);
int32_t SystemNative_WaitPidExitedNoHang(int32_t pid, int32_t* exitCode, int32_t* terminatingSignal);
/* pal_runtimeinformation.h */
int32_t SystemNative_GetOSArchitecture(void);
char* SystemNative_GetUnixRelease(void);
int32_t SystemNative_GetUnixVersion(char* version, int* capacity);
/* pal_searchpath.h */
const char* SystemNative_SearchPath(int32_t folderId);
const char* SystemNative_SearchPath_TempDirectory(void);
/* pal_string.h */
int32_t SystemNative_SNPrintF(char* string, int32_t size, const char* format, ...);
int32_t SystemNative_SNPrintF_1I(char* string, int32_t size, const char* format, int arg);
int32_t SystemNative_SNPrintF_1S(char* string, int32_t size, const char* format, char* str);
/* pal_sysctl.h */
int32_t SystemNative_Sysctl(int* name, unsigned int namelen, void* value, size_t* len);
/* pal_tcpstate.h */
int32_t SystemNative_MapTcpState(int32_t tcpState);
/* pal_time.h */
int64_t SystemNative_GetBootTimeTicks(void);
/* pal_uid.h */
uint32_t SystemNative_GetEGid(void);
int32_t SystemNative_GetGroupList(const char* name, uint32_t group, uint32_t* groups, int32_t* ngroups);
char* SystemNative_GetGroupName(uint32_t gid);
int32_t SystemNative_GetGroups(int32_t ngroups, uint32_t* groups);
int32_t SystemNative_GetPwNamR(const char* name, rhu_pal_passwd* pwd, char* buf, int32_t buflen);
int32_t SystemNative_SetEUid(uint32_t euid);

/* libSystem.Globalization.Native.a: GlobalizationNative_ */
/* pal_calendarData.h */
int32_t GlobalizationNative_EnumCalendarInfo(EnumCalendarInfoCallback callback, const uint16_t* localeName, uint16_t calendarId, int32_t dataType, const void* context);
int32_t GlobalizationNative_GetCalendarInfo(const uint16_t* localeName, uint16_t calendarId, int32_t dataType, uint16_t* result, int32_t resultCapacity);
int32_t GlobalizationNative_GetCalendars(const uint16_t* localeName, uint16_t* calendars, int32_t calendarsCapacity);
int32_t GlobalizationNative_GetJapaneseEraStartDate(int32_t era, int32_t* startYear, int32_t* startMonth, int32_t* startDay);
int32_t GlobalizationNative_GetLatestJapaneseEra(void);
/* pal_casing.h */
void GlobalizationNative_ChangeCase(const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength, int32_t bToUpper);
void GlobalizationNative_ChangeCaseInvariant(const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength, int32_t bToUpper);
void GlobalizationNative_ChangeCaseTurkish(const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength, int32_t bToUpper);
void GlobalizationNative_InitOrdinalCasingPage(int32_t pageNumber, uint16_t* pTarget);
void GlobalizationNative_InitOrdinalLowerCasingPage(int32_t pageNumber, uint16_t* pTarget);
/* pal_collation.h */
void GlobalizationNative_CloseSortHandle(SortHandle* pSortHandle);
int32_t GlobalizationNative_CompareString(SortHandle* pSortHandle, const uint16_t* lpStr1, int32_t cwStr1Length, const uint16_t* lpStr2, int32_t cwStr2Length, int32_t options);
int32_t GlobalizationNative_EndsWith(SortHandle* pSortHandle, const uint16_t* lpTarget, int32_t cwTargetLength, const uint16_t* lpSource, int32_t cwSourceLength, int32_t options, int32_t* pMatchedLength);
int32_t GlobalizationNative_GetSortHandle(const char* lpLocaleName, SortHandle** ppSortHandle);
int32_t GlobalizationNative_GetSortKey(SortHandle* pSortHandle, const uint16_t* lpStr, int32_t cwStrLength, uint8_t* sortKey, int32_t cbSortKeyLength, int32_t options);
int32_t GlobalizationNative_GetSortVersion(SortHandle* pSortHandle);
int32_t GlobalizationNative_IndexOf(SortHandle* pSortHandle, const uint16_t* lpTarget, int32_t cwTargetLength, const uint16_t* lpSource, int32_t cwSourceLength, int32_t options, int32_t* pMatchedLength);
int32_t GlobalizationNative_LastIndexOf(SortHandle* pSortHandle, const uint16_t* lpTarget, int32_t cwTargetLength, const uint16_t* lpSource, int32_t cwSourceLength, int32_t options, int32_t* pMatchedLength);
int32_t GlobalizationNative_StartsWith(SortHandle* pSortHandle, const uint16_t* lpTarget, int32_t cwTargetLength, const uint16_t* lpSource, int32_t cwSourceLength, int32_t options, int32_t* pMatchedLength);
/* pal_icushim.h */
int32_t GlobalizationNative_GetICUVersion(void);
void GlobalizationNative_InitICUFunctions(void* icuuc, void* icuin, const char* version, const char* suffix);
int32_t GlobalizationNative_LoadICU(void);
/* pal_idna.h */
int32_t GlobalizationNative_ToAscii(uint32_t flags, const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength);
int32_t GlobalizationNative_ToUnicode(uint32_t flags, const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength);
/* pal_locale.h */
int32_t GlobalizationNative_GetDefaultLocaleName(uint16_t* value, int32_t valueLength);
int32_t GlobalizationNative_GetLocaleName(const uint16_t* localeName, uint16_t* value, int32_t valueLength);
int32_t GlobalizationNative_GetLocaleTimeFormat(const uint16_t* localeName, int shortFormat, uint16_t* value, int32_t valueLength);
int32_t GlobalizationNative_GetLocales(uint16_t* value, int32_t valueLength);
int32_t GlobalizationNative_IsPredefinedLocale(const uint16_t* localeName);
/* pal_localeNumberData.h */
int32_t GlobalizationNative_GetLocaleInfoGroupingSizes(const uint16_t* localeName, int32_t localeGroupingData, int32_t* primaryGroupSize, int32_t* secondaryGroupSize);
int32_t GlobalizationNative_GetLocaleInfoInt(const uint16_t* localeName, int32_t localeNumberData, int32_t* value);
/* pal_localeStringData.h */
int32_t GlobalizationNative_GetLocaleInfoString(const uint16_t* localeName, int32_t localeStringData, uint16_t* value, int32_t valueLength, const uint16_t* uiLocaleName);
/* pal_normalization.h */
int32_t GlobalizationNative_IsNormalized(int32_t normalizationForm, const uint16_t* lpStr, int32_t cwStrLength);
int32_t GlobalizationNative_NormalizeString(int32_t normalizationForm, const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength);
/* pal_timeZoneInfo.h */
int32_t GlobalizationNative_GetTimeZoneDisplayName(const uint16_t* localeName, const uint16_t* timeZoneId, int32_t type, uint16_t* result, int32_t resultLength);
int32_t GlobalizationNative_IanaIdToWindowsId(const uint16_t* ianaId, uint16_t* windowsId, int32_t windowsIdLength);
int32_t GlobalizationNative_WindowsIdToIanaId(const uint16_t* windowsId, const char* region, uint16_t* ianaId, int32_t ianaIdLength);

/* libSystem.IO.Compression.Native.a: CompressionNative_ */
/* pal_zlib.h */
uint32_t CompressionNative_CompressBound(uint32_t sourceLen);
uint32_t CompressionNative_Crc32(uint32_t crc, uint8_t* buffer, int32_t len);
int32_t CompressionNative_Deflate(PAL_ZStream* stream, int32_t flush);
int32_t CompressionNative_DeflateEnd(PAL_ZStream* stream);
int32_t CompressionNative_DeflateInit2_(PAL_ZStream* stream, int32_t level, int32_t method, int32_t windowBits, int32_t memLevel, int32_t strategy);
int32_t CompressionNative_DeflateReset(PAL_ZStream* stream);
int32_t CompressionNative_Inflate(PAL_ZStream* stream, int32_t flush);
int32_t CompressionNative_InflateEnd(PAL_ZStream* stream);
int32_t CompressionNative_InflateInit2_(PAL_ZStream* stream, int32_t windowBits);
int32_t CompressionNative_InflateReset2_(PAL_ZStream* stream, int32_t windowBits);

/* libSystem.Net.Security.Native.a: NetSecurityNative_ */
/* pal_gssapi.h */
uint32_t NetSecurityNative_AcceptSecContext(uint32_t* minorStatus, GssCredId* acceptorCredHandle, GssCtxId** contextHandle, void* cbt, int32_t cbtSize, uint8_t* inputBytes, uint32_t inputLength, PAL_GssBuffer* outBuffer, uint32_t* retFlags, int32_t* isNtlmUsed);
uint32_t NetSecurityNative_AcquireAcceptorCred(uint32_t* minorStatus, GssCredId** outputCredHandle);
uint32_t NetSecurityNative_DeleteSecContext(uint32_t* minorStatus, GssCtxId** contextHandle);
uint32_t NetSecurityNative_DisplayMajorStatus(uint32_t* minorStatus, uint32_t statusValue, PAL_GssBuffer* outBuffer);
uint32_t NetSecurityNative_DisplayMinorStatus(uint32_t* minorStatus, uint32_t statusValue, PAL_GssBuffer* outBuffer);
int32_t NetSecurityNative_EnsureGssInitialized(void);
uint32_t NetSecurityNative_GetMic(uint32_t* minorStatus, GssCtxId* contextHandle, uint8_t* inputBytes, int32_t inputLength, PAL_GssBuffer* outBuffer);
uint32_t NetSecurityNative_GetUser(uint32_t* minorStatus, GssCtxId* contextHandle, PAL_GssBuffer* outBuffer);
uint32_t NetSecurityNative_ImportPrincipalName(uint32_t* minorStatus, char* inputName, uint32_t inputNameLen, GssName** outputName);
uint32_t NetSecurityNative_ImportUserName(uint32_t* minorStatus, char* inputName, uint32_t inputNameLen, GssName** outputName);
uint32_t NetSecurityNative_InitSecContext(uint32_t* minorStatus, GssCredId* claimantCredHandle, GssCtxId** contextHandle, uint32_t packageType, GssName* targetName, uint32_t reqFlags, uint8_t* inputBytes, uint32_t inputLength, PAL_GssBuffer* outBuffer, uint32_t* retFlags, int32_t* isNtlmUsed);
uint32_t NetSecurityNative_InitSecContextEx(uint32_t* minorStatus, GssCredId* claimantCredHandle, GssCtxId** contextHandle, uint32_t packageType, void* cbt, int32_t cbtSize, GssName* targetName, uint32_t reqFlags, uint8_t* inputBytes, uint32_t inputLength, PAL_GssBuffer* outBuffer, uint32_t* retFlags, int32_t* isNtlmUsed);
uint32_t NetSecurityNative_InitiateCredSpNego(uint32_t* minorStatus, GssName* desiredName, GssCredId** outputCredHandle);
uint32_t NetSecurityNative_InitiateCredWithPassword(uint32_t* minorStatus, int32_t packageType, GssName* desiredName, char* password, uint32_t passwdLen, GssCredId** outputCredHandle);
uint32_t NetSecurityNative_IsNtlmInstalled(void);
uint32_t NetSecurityNative_ReleaseCred(uint32_t* minorStatus, GssCredId** credHandle);
void NetSecurityNative_ReleaseGssBuffer(void* buffer, uint64_t length);
uint32_t NetSecurityNative_ReleaseName(uint32_t* minorStatus, GssName** inputName);
uint32_t NetSecurityNative_Unwrap(uint32_t* minorStatus, GssCtxId* contextHandle, int32_t* isEncrypt, uint8_t* inputBytes, int32_t count, PAL_GssBuffer* outBuffer);
uint32_t NetSecurityNative_VerifyMic(uint32_t* minorStatus, GssCtxId* contextHandle, uint8_t* inputBytes, int32_t inputLength, uint8_t* tokenBytes, int32_t tokenLength);
uint32_t NetSecurityNative_Wrap(uint32_t* minorStatus, GssCtxId* contextHandle, int32_t* isEncrypt, uint8_t* inputBytes, int32_t count, PAL_GssBuffer* outBuffer);

/* libSystem.Security.Cryptography.Native.OpenSsl.a: CryptoNative_ */
/* memory_debug.h */
void CryptoNative_EnableMemoryTracking(int32_t enable);
void CryptoNative_ForEachTrackedAllocation(void (*callback)(void* ptr, uint64_t size, const char* file, int32_t line, void* ctx), void* ctx);
void CryptoNative_GetMemoryUse(uint64_t* totalUsed, uint64_t* allocationCount);
/* openssl.h */
int32_t CryptoNative_BioSeek(BIO* bio, int32_t ofs);
int32_t CryptoNative_BioTell(BIO* bio);
int32_t CryptoNative_CheckX509Hostname(X509* x509, const char* hostname, int32_t cchHostname);
int32_t CryptoNative_CheckX509IpAddress(X509* x509, const uint8_t* addressBytes, int32_t addressBytesLen, const char* hostname, int32_t cchHostname);
int32_t CryptoNative_EnsureOpenSslInitialized(void);
int32_t CryptoNative_GetAsn1StringBytes(ASN1_STRING* asn1, uint8_t* pBuf, int32_t cBuf);
int32_t CryptoNative_GetRandomBytes(uint8_t* buf, int32_t num);
const ASN1_TIME* CryptoNative_GetX509CrlNextUpdate(X509_CRL* crl);
BIO* CryptoNative_GetX509NameInfo(X509* x509, int32_t nameType, int32_t forIssuer);
int32_t CryptoNative_GetX509NameRawBytes(X509_NAME* x509Name, uint8_t* pBuf, int32_t cBuf);
const ASN1_TIME* CryptoNative_GetX509NotAfter(X509* x509);
const ASN1_TIME* CryptoNative_GetX509NotBefore(X509* x509);
ASN1_OBJECT* CryptoNative_GetX509PublicKeyAlgorithm(X509* x509);
const ASN1_BIT_STRING* CryptoNative_GetX509PublicKeyBytes(X509* x509);
int32_t CryptoNative_GetX509PublicKeyParameterBytes(X509* x509, uint8_t* pBuf, int32_t cBuf);
ASN1_OBJECT* CryptoNative_GetX509SignatureAlgorithm(X509* x509);
X509* CryptoNative_GetX509StackField(X509_stack* stack, int loc);
int32_t CryptoNative_GetX509StackFieldCount(X509_stack* stack);
int32_t CryptoNative_GetX509Thumbprint(X509* x509, uint8_t* pBuf, int32_t cBuf);
int32_t CryptoNative_GetX509Version(X509* x509);
int32_t CryptoNative_IsSignatureAlgorithmAvailable(const char* algorithm);
int32_t CryptoNative_LookupFriendlyNameByOid(const char* oidValue, const char** friendlyName);
X509_stack* CryptoNative_NewX509Stack(void);
int32_t CryptoNative_OpenSslAvailable(void);
int64_t CryptoNative_OpenSslVersionNumber(void);
int32_t CryptoNative_PushX509StackField(X509_stack* stack, X509* x509);
X509* CryptoNative_ReadX509AsDerFromBio(BIO* bio);
void CryptoNative_RecursiveFreeX509Stack(X509_stack* stack);
void CryptoNative_RegisterLegacyAlgorithms(void);
int32_t CryptoNative_X509StoreSetVerifyTime(X509_STORE* ctx, int64_t unixTime);
/* pal_asn1.h */
void CryptoNative_Asn1ObjectFree(ASN1_OBJECT* a);
void CryptoNative_Asn1OctetStringFree(ASN1_STRING* a);
ASN1_OCTET_STRING* CryptoNative_Asn1OctetStringNew(void);
int32_t CryptoNative_Asn1OctetStringSet(ASN1_OCTET_STRING* s, const uint8_t* data, int32_t len);
int32_t CryptoNative_EncodeAsn1Integer(ASN1_INTEGER* i, uint8_t* buf);
int32_t CryptoNative_GetAsn1IntegerDerSize(ASN1_INTEGER* i);
const ASN1_OBJECT* CryptoNative_GetObjectDefinitionByName(const char* friendlyName);
const ASN1_OBJECT* CryptoNative_ObjNid2Obj(int32_t nid);
int32_t CryptoNative_ObjObj2Txt(char* buf, int32_t buf_len, const ASN1_OBJECT* a);
int32_t CryptoNative_ObjTxt2Nid(const char* sn);
const ASN1_OBJECT* CryptoNative_ObjTxt2Obj(const char* s);
/* pal_bignum.h */
void CryptoNative_BigNumDestroy(BIGNUM* a);
int32_t CryptoNative_BigNumToBinary(const BIGNUM* a, uint8_t* to);
int32_t CryptoNative_GetBigNumBytes(const BIGNUM* a);
/* pal_bio.h */
void CryptoNative_BioClearReadWindow(BIO* bio, int32_t* leftoverLength);
int32_t CryptoNative_BioDestroy(BIO* a);
int32_t CryptoNative_BioDrainSpill(BIO* bio, void* dst, int32_t dstLen);
int32_t CryptoNative_BioGetReplayPrefix(BIO* bio, uint8_t** outPtr, int32_t* outLen);
void CryptoNative_BioGetWriteResult(BIO* bio, int32_t* writtenToWindow, int32_t* spillLen);
int32_t CryptoNative_BioGets(BIO* b, char* buf, int32_t size);
BIO* CryptoNative_BioNewFile(const char* filename, const char* mode);
BIO* CryptoNative_BioNewManagedSpan(void);
BIO* CryptoNative_BioNewSocketReplay(intptr_t fd, const void* prefix, int32_t prefixLen);
int32_t CryptoNative_BioPeekTlsFrame(BIO* bio, uint8_t** outPtr, int32_t* outLen);
int32_t CryptoNative_BioRead(BIO* b, void* buf, int32_t len);
void CryptoNative_BioSetReadWindow(BIO* bio, const void* ptr, int32_t len);
void CryptoNative_BioSetWriteWindow(BIO* bio, void* ptr, int32_t capacity);
int32_t CryptoNative_BioWrite(BIO* b, const void* buf, int32_t len);
BIO* CryptoNative_CreateMemoryBio(void);
int32_t CryptoNative_GetMemoryBioSize(BIO* bio);
/* pal_dsa.h */
void CryptoNative_DsaDestroy(DSA* dsa);
int32_t CryptoNative_DsaGenerateKey(DSA** dsa, int32_t bits);
int32_t CryptoNative_DsaKeyCreateByExplicitParameters(DSA** dsa, uint8_t* p, int32_t pLength, uint8_t* q, int32_t qLength, uint8_t* g, int32_t gLength, uint8_t* y, int32_t yLength, uint8_t* x, int32_t xLength);
int32_t CryptoNative_DsaSign(DSA* dsa, const uint8_t* hash, int32_t hashLength, uint8_t* signature, int32_t* outSignatureLength);
int32_t CryptoNative_DsaSizeP(DSA* dsa);
int32_t CryptoNative_DsaSizeQ(DSA* dsa);
int32_t CryptoNative_DsaSizeSignature(DSA* dsa);
int32_t CryptoNative_DsaUpRef(DSA* dsa);
int32_t CryptoNative_DsaVerify(DSA* dsa, const uint8_t* hash, int32_t hashLength, uint8_t* signature, int32_t signatureLength);
int32_t CryptoNative_GetDsaParameters(const DSA* dsa, const BIGNUM** p, int32_t* pLength, const BIGNUM** q, int32_t* qLength, const BIGNUM** g, int32_t* gLength, const BIGNUM** y, int32_t* yLength, const BIGNUM** x, int32_t* xLength);
/* pal_ecc_import_export.h */
EC_KEY* CryptoNative_EcKeyCreateByExplicitParameters(int32_t curveType, const uint8_t* qx, int32_t qxLength, const uint8_t* qy, int32_t qyLength, const uint8_t* d, int32_t dLength, const uint8_t* p, int32_t pLength, const uint8_t* a, int32_t aLength, const uint8_t* b, int32_t bLength, const uint8_t* gx, int32_t gxLength, const uint8_t* gy, int32_t gyLength, const uint8_t* order, int32_t nLength, const uint8_t* cofactor, int32_t hLength, const uint8_t* seed, int32_t sLength);
int32_t CryptoNative_EcKeyCreateByKeyParameters(EC_KEY** key, const char* oid, const uint8_t* qx, int32_t qxLength, const uint8_t* qy, int32_t qyLength, const uint8_t* d, int32_t dLength);
int32_t CryptoNative_EvpPKeyCreateByEcExplicitParameters(int32_t curveType, const uint8_t* qx, int32_t qxLength, const uint8_t* qy, int32_t qyLength, const uint8_t* d, int32_t dLength, const uint8_t* p, int32_t pLength, const uint8_t* a, int32_t aLength, const uint8_t* b, int32_t bLength, const uint8_t* gx, int32_t gxLength, const uint8_t* gy, int32_t gyLength, const uint8_t* order, int32_t orderLength, const uint8_t* cofactor, int32_t cofactorLength, const uint8_t* seed, int32_t seedLength, EVP_PKEY** pkey, int32_t* keySize);
int32_t CryptoNative_EvpPKeyCreateByEcParameters(EVP_PKEY** pkey, const char* oid, const uint8_t* qx, int32_t qxLength, const uint8_t* qy, int32_t qyLength, const uint8_t* d, int32_t dLength, int32_t* keySize);
int32_t CryptoNative_EvpPKeyEcHasExplicitEncoding(EVP_PKEY* pkey);
int32_t CryptoNative_EvpPKeyGenerateByEcCurveOid(EVP_PKEY** pkey, const char* oid, int32_t* keySize);
int32_t CryptoNative_EvpPKeyGetEcCurveParameters(EVP_PKEY* pkey, int32_t includePrivate, int32_t* curveType, BIGNUM** qx, int32_t* cbQx, BIGNUM** qy, int32_t* cbQy, BIGNUM** d, int32_t* cbD, BIGNUM** p, int32_t* cbP, BIGNUM** a, int32_t* cbA, BIGNUM** b, int32_t* cbB, BIGNUM** gx, int32_t* cbGx, BIGNUM** gy, int32_t* cbGy, BIGNUM** order, int32_t* cbOrder, BIGNUM** cofactor, int32_t* cbCofactor, BIGNUM** seed, int32_t* cbSeed);
int32_t CryptoNative_EvpPKeyGetEcGroupNid(EVP_PKEY* pkey, int32_t* nidName);
int32_t CryptoNative_EvpPKeyGetEcKeyParameters(EVP_PKEY* pkey, int32_t includePrivate, BIGNUM** qx, int32_t* cbQx, BIGNUM** qy, int32_t* cbQy, BIGNUM** d, int32_t* cbD);
int32_t CryptoNative_EvpPKeyGetEcKeySize(EVP_PKEY* pkey);
/* pal_eckey.h */
EC_KEY* CryptoNative_EcKeyCreateByOid(const char* oid);
void CryptoNative_EcKeyDestroy(EC_KEY* r);
int32_t CryptoNative_EcKeyGenerateKey(EC_KEY* eckey);
int32_t CryptoNative_EcKeyGetCurveName2(const EC_KEY* key, int32_t* nidName);
int32_t CryptoNative_EcKeyGetSize(const EC_KEY* key, int32_t* keySize);
int32_t CryptoNative_EcKeyUpRef(EC_KEY* r);
/* pal_err.h */
void CryptoNative_ErrClearError(void);
void CryptoNative_ErrErrorStringN(uint64_t e, char* buf, int32_t len);
uint64_t CryptoNative_ErrGetExceptionError(int32_t* isAllocFailure);
uint64_t CryptoNative_ErrPeekError(void);
uint64_t CryptoNative_ErrPeekLastError(void);
const char* CryptoNative_ErrReasonErrorString(uint64_t error);
/* pal_evp.h */
int32_t CryptoNative_EvpDigestCurrent(const EVP_MD_CTX* ctx, uint8_t* md, uint32_t* s);
int32_t CryptoNative_EvpDigestCurrentXOF(const EVP_MD_CTX* ctx, uint8_t* md, uint32_t len);
int32_t CryptoNative_EvpDigestFinalEx(EVP_MD_CTX* ctx, uint8_t* md, uint32_t* s);
int32_t CryptoNative_EvpDigestFinalXOF(EVP_MD_CTX* ctx, uint8_t* md, uint32_t len);
int32_t CryptoNative_EvpDigestOneShot(const EVP_MD* type, const void* source, int32_t sourceSize, uint8_t* md, uint32_t* mdSize);
int32_t CryptoNative_EvpDigestReset(EVP_MD_CTX* ctx, const EVP_MD* type);
int32_t CryptoNative_EvpDigestSqueeze(EVP_MD_CTX* ctx, uint8_t* md, uint32_t len, int32_t* haveFeature);
int32_t CryptoNative_EvpDigestUpdate(EVP_MD_CTX* ctx, const void* d, int32_t cnt);
int32_t CryptoNative_EvpDigestXOFOneShot(const EVP_MD* type, const void* source, int32_t sourceSize, uint8_t* md, uint32_t len);
const EVP_MD* CryptoNative_EvpMd5(void);
EVP_MD_CTX* CryptoNative_EvpMdCtxCopyEx(const EVP_MD_CTX* ctx);
EVP_MD_CTX* CryptoNative_EvpMdCtxCreate(const EVP_MD* type);
void CryptoNative_EvpMdCtxDestroy(EVP_MD_CTX* ctx);
int32_t CryptoNative_EvpMdSize(const EVP_MD* md);
const EVP_MD* CryptoNative_EvpSha1(void);
const EVP_MD* CryptoNative_EvpSha256(void);
const EVP_MD* CryptoNative_EvpSha384(void);
const EVP_MD* CryptoNative_EvpSha3_256(void);
const EVP_MD* CryptoNative_EvpSha3_384(void);
const EVP_MD* CryptoNative_EvpSha3_512(void);
const EVP_MD* CryptoNative_EvpSha512(void);
const EVP_MD* CryptoNative_EvpShake128(void);
const EVP_MD* CryptoNative_EvpShake256(void);
int32_t CryptoNative_GetMaxMdSize(void);
int32_t CryptoNative_Pbkdf2(const char* password, int32_t passwordLength, const unsigned char* salt, int32_t saltLength, int32_t iterations, const EVP_MD* digest, unsigned char* destination, int32_t destinationLength);
/* pal_evp_cipher.h */
const EVP_CIPHER* CryptoNative_EvpAes128Cbc(void);
const EVP_CIPHER* CryptoNative_EvpAes128Ccm(void);
const EVP_CIPHER* CryptoNative_EvpAes128Cfb128(void);
const EVP_CIPHER* CryptoNative_EvpAes128Cfb8(void);
const EVP_CIPHER* CryptoNative_EvpAes128Ecb(void);
const EVP_CIPHER* CryptoNative_EvpAes128Gcm(void);
const EVP_CIPHER* CryptoNative_EvpAes128Wrap(void);
const EVP_CIPHER* CryptoNative_EvpAes128WrapPad(void);
const EVP_CIPHER* CryptoNative_EvpAes192Cbc(void);
const EVP_CIPHER* CryptoNative_EvpAes192Ccm(void);
const EVP_CIPHER* CryptoNative_EvpAes192Cfb128(void);
const EVP_CIPHER* CryptoNative_EvpAes192Cfb8(void);
const EVP_CIPHER* CryptoNative_EvpAes192Ecb(void);
const EVP_CIPHER* CryptoNative_EvpAes192Gcm(void);
const EVP_CIPHER* CryptoNative_EvpAes192Wrap(void);
const EVP_CIPHER* CryptoNative_EvpAes192WrapPad(void);
const EVP_CIPHER* CryptoNative_EvpAes256Cbc(void);
const EVP_CIPHER* CryptoNative_EvpAes256Ccm(void);
const EVP_CIPHER* CryptoNative_EvpAes256Cfb128(void);
const EVP_CIPHER* CryptoNative_EvpAes256Cfb8(void);
const EVP_CIPHER* CryptoNative_EvpAes256Ecb(void);
const EVP_CIPHER* CryptoNative_EvpAes256Gcm(void);
const EVP_CIPHER* CryptoNative_EvpAes256Wrap(void);
const EVP_CIPHER* CryptoNative_EvpAes256WrapPad(void);
const EVP_CIPHER* CryptoNative_EvpChaCha20Poly1305(void);
EVP_CIPHER_CTX* CryptoNative_EvpCipherCreate2(const EVP_CIPHER* type, uint8_t* key, int32_t keyLength, unsigned char* iv, int32_t enc);
EVP_CIPHER_CTX* CryptoNative_EvpCipherCreatePartial(const EVP_CIPHER* type);
int32_t CryptoNative_EvpCipherCtxSetPadding(EVP_CIPHER_CTX* x, int32_t padding);
void CryptoNative_EvpCipherDestroy(EVP_CIPHER_CTX* ctx);
int32_t CryptoNative_EvpCipherFinalEx(EVP_CIPHER_CTX* ctx, uint8_t* outm, int32_t* outl);
int32_t CryptoNative_EvpCipherGetAeadTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength);
int32_t CryptoNative_EvpCipherGetCcmTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength);
int32_t CryptoNative_EvpCipherGetGcmTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength);
int32_t CryptoNative_EvpCipherReset(EVP_CIPHER_CTX* ctx, uint8_t* pIv, int32_t cIv);
int32_t CryptoNative_EvpCipherSetAeadTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength);
int32_t CryptoNative_EvpCipherSetCcmNonceLength(EVP_CIPHER_CTX* ctx, int32_t ivLength);
int32_t CryptoNative_EvpCipherSetCcmTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength);
int32_t CryptoNative_EvpCipherSetGcmNonceLength(EVP_CIPHER_CTX* ctx, int32_t ivLength);
int32_t CryptoNative_EvpCipherSetGcmTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength);
int32_t CryptoNative_EvpCipherSetKeyAndIV(EVP_CIPHER_CTX* ctx, uint8_t* key, unsigned char* iv, int32_t enc);
int32_t CryptoNative_EvpCipherUpdate(EVP_CIPHER_CTX* ctx, uint8_t* out, int32_t* outl, unsigned char* in, int32_t inl);
const EVP_CIPHER* CryptoNative_EvpDes3Cbc(void);
const EVP_CIPHER* CryptoNative_EvpDes3Cfb64(void);
const EVP_CIPHER* CryptoNative_EvpDes3Cfb8(void);
const EVP_CIPHER* CryptoNative_EvpDes3Ecb(void);
const EVP_CIPHER* CryptoNative_EvpDesCbc(void);
const EVP_CIPHER* CryptoNative_EvpDesCfb8(void);
const EVP_CIPHER* CryptoNative_EvpDesEcb(void);
const EVP_CIPHER* CryptoNative_EvpRC2Cbc(void);
const EVP_CIPHER* CryptoNative_EvpRC2Ecb(void);
/* pal_evp_kdf.h */
EVP_KDF* CryptoNative_EvpKdfFetch(const char* algorithm, int32_t* haveFeature);
void CryptoNative_EvpKdfFree(EVP_KDF* kdf);
int32_t CryptoNative_HkdfDeriveKey(EVP_KDF* kdf, uint8_t* ikm, int32_t ikmLength, char* algorithm, uint8_t* salt, int32_t saltLength, uint8_t* info, int32_t infoLength, uint8_t* destination, int32_t destinationLength);
int32_t CryptoNative_HkdfExpand(EVP_KDF* kdf, uint8_t* prk, int32_t prkLength, char* algorithm, uint8_t* info, int32_t infoLength, uint8_t* destination, int32_t destinationLength);
int32_t CryptoNative_HkdfExtract(EVP_KDF* kdf, uint8_t* ikm, int32_t ikmLength, char* algorithm, uint8_t* salt, int32_t saltLength, uint8_t* destination, int32_t destinationLength);
int32_t CryptoNative_KbkdfHmacOneShot(EVP_KDF* kdf, uint8_t* key, int32_t keyLength, char* algorithm, uint8_t* label, int32_t labelLength, uint8_t* context, int32_t contextLength, uint8_t* destination, int32_t destinationLength);
/* pal_evp_kem.h */
int32_t CryptoNative_EvpKemAvailable(const char* algorithm);
int32_t CryptoNative_EvpKemDecapsulate(EVP_PKEY* pKey, void* extraHandle, const uint8_t* ciphertext, int32_t ciphertextLength, uint8_t* sharedSecret, int32_t sharedSecretLength);
int32_t CryptoNative_EvpKemEncapsulate(EVP_PKEY* pKey, void* extraHandle, uint8_t* ciphertext, int32_t ciphertextLength, uint8_t* sharedSecret, int32_t sharedSecretLength);
int32_t CryptoNative_EvpKemExportDecapsulationKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength);
int32_t CryptoNative_EvpKemExportEncapsulationKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength);
int32_t CryptoNative_EvpKemExportPrivateSeed(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength);
EVP_PKEY* CryptoNative_EvpKemGeneratePkey(const char* kemName, uint8_t* seed, int32_t seedLength);
int32_t CryptoNative_EvpKemGetPalId(const EVP_PKEY* pKey, int32_t* kemId, int32_t* hasSeed, int32_t* hasDecapsulationKey);
/* pal_evp_mac.h */
EVP_MAC_CTX* CryptoNative_EvpMacCtxDup(const EVP_MAC_CTX* ctx);
void CryptoNative_EvpMacCtxFree(EVP_MAC_CTX* ctx);
EVP_MAC_CTX* CryptoNative_EvpMacCtxNew(EVP_MAC* mac);
int32_t CryptoNative_EvpMacCurrent(EVP_MAC_CTX* ctx, uint8_t* mac, int32_t macLength);
EVP_MAC* CryptoNative_EvpMacFetch(const char* algorithm, int32_t* haveFeature);
int32_t CryptoNative_EvpMacFinal(EVP_MAC_CTX* ctx, uint8_t* mac, int32_t macLength);
void CryptoNative_EvpMacFree(EVP_MAC* mac);
int32_t CryptoNative_EvpMacInit(EVP_MAC_CTX* ctx, uint8_t* key, int32_t keyLength, uint8_t* customizationString, int32_t customizationStringLength, int32_t xof);
int32_t CryptoNative_EvpMacOneShot(EVP_MAC* mac, uint8_t* key, int32_t keyLength, uint8_t* customizationString, int32_t customizationStringLength, const uint8_t* data, int32_t dataLength, uint8_t* destination, int32_t destinationLength, int32_t xof);
int32_t CryptoNative_EvpMacReset(EVP_MAC_CTX* ctx);
int32_t CryptoNative_EvpMacUpdate(EVP_MAC_CTX* ctx, uint8_t* data, int32_t dataLength);
/* pal_evp_pkey.h */
EVP_PKEY* CryptoNative_DecodePkcs8PrivateKey(const uint8_t* buf, int32_t len, int32_t algId);
EVP_PKEY* CryptoNative_DecodeSubjectPublicKeyInfo(const uint8_t* buf, int32_t len, int32_t algId);
int32_t CryptoNative_EncodePkcs8PrivateKey(EVP_PKEY* pkey, uint8_t* buf);
int32_t CryptoNative_EncodeSubjectPublicKeyInfo(EVP_PKEY* pkey, uint8_t* buf);
int32_t CryptoNative_EvpPKeyBits(EVP_PKEY* pkey);
int32_t CryptoNative_EvpPKeyFamily(const EVP_PKEY* key);
EVP_PKEY* CryptoNative_EvpPKeyFromData(const char* algorithmName, uint8_t* key, int32_t keyLength, int32_t privateKey);
int32_t CryptoNative_EvpPKeyType(EVP_PKEY* key);
EVP_PKEY* CryptoNative_EvpPkeyCreate(void);
void CryptoNative_EvpPkeyDestroy(EVP_PKEY* pkey);
int32_t CryptoNative_GetPkcs8PrivateKeySize(EVP_PKEY* pkey, int32_t* p8size);
int32_t CryptoNative_GetSubjectPublicKeyInfoSize(EVP_PKEY* pkey);
EVP_PKEY* CryptoNative_LoadKeyFromProvider(const char** providerNames, int32_t providerNameCount, const char* keyUri, const char* propertyQuery, void** extraHandle, int32_t* haveProvider);
EVP_PKEY* CryptoNative_LoadPrivateKeyFromEngine(const char* engineName, const char* keyName, int32_t* haveEngine);
EVP_PKEY* CryptoNative_LoadPublicKeyFromEngine(const char* engineName, const char* keyName, int32_t* haveEngine);
int32_t CryptoNative_UpRefEvpPkey(EVP_PKEY* pkey);
/* pal_evp_pkey_dsa.h */
DSA* CryptoNative_EvpPkeyGetDsa(EVP_PKEY* pkey);
int32_t CryptoNative_EvpPkeySetDsa(EVP_PKEY* pkey, DSA* dsa);
/* pal_evp_pkey_ecdh.h */
int32_t CryptoNative_EvpPKeyDeriveSecretAgreement(EVP_PKEY* pkey, void* extraHandle, EVP_PKEY* peerKey, uint8_t* secret, uint32_t secretLength);
/* pal_evp_pkey_ecdsa.h */
int32_t CryptoNative_EcDsaSignHash(EVP_PKEY* pkey, void* extraHandle, const uint8_t* hash, int32_t hashLen, uint8_t* destination, int32_t destinationLen);
int32_t CryptoNative_EcDsaVerifyHash(EVP_PKEY* pkey, void* extraHandle, const uint8_t* hash, int32_t hashLen, const uint8_t* signature, int32_t signatureLen);
/* pal_evp_pkey_eckey.h */
EVP_PKEY* CryptoNative_CreateEvpPkeyFromEcKey(EC_KEY* ecKey, int32_t* outKeySize);
/* pal_evp_pkey_ml_dsa.h */
int32_t CryptoNative_MLDsaExportPublicKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength);
int32_t CryptoNative_MLDsaExportSecretKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength);
int32_t CryptoNative_MLDsaExportSeed(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength);
EVP_PKEY* CryptoNative_MLDsaGenerateKey(const char* keyType, uint8_t* seed, int32_t seedLen);
int32_t CryptoNative_MLDsaGetPalId(const EVP_PKEY* pKey, int32_t* mldsaId, int32_t* hasSeed, int32_t* hasSecretKey);
int32_t CryptoNative_MLDsaSignExternalMu(EVP_PKEY* pKey, void* extraHandle, uint8_t* mu, int32_t muLen, uint8_t* destination, int32_t destinationLen);
int32_t CryptoNative_MLDsaSignPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* destination, int32_t destinationLen);
int32_t CryptoNative_MLDsaSignPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* destination, int32_t destinationLen);
int32_t CryptoNative_MLDsaVerifyExternalMu(EVP_PKEY* pKey, void* extraHandle, uint8_t* mu, int32_t muLen, uint8_t* sig, int32_t sigLen);
int32_t CryptoNative_MLDsaVerifyPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* sig, int32_t sigLen);
int32_t CryptoNative_MLDsaVerifyPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* sig, int32_t sigLen);
/* pal_evp_pkey_raw_signverify.h */
int32_t CryptoNative_EvpPKeySignPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* destination, int32_t destinationLen);
int32_t CryptoNative_EvpPKeySignPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* destination, int32_t destinationLen);
int32_t CryptoNative_EvpPKeyVerifyPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* sig, int32_t sigLen);
int32_t CryptoNative_EvpPKeyVerifyPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* sig, int32_t sigLen);
/* pal_evp_pkey_rsa.h */
EVP_PKEY* CryptoNative_EvpPKeyCreateRsa(RSA* currentKey);
int32_t CryptoNative_RsaDecrypt(EVP_PKEY* pkey, void* extraHandle, const uint8_t* source, int32_t sourceLen, int32_t padding, const EVP_MD* digest, uint8_t* destination, int32_t destinationLen);
int32_t CryptoNative_RsaEncrypt(EVP_PKEY* pkey, void* extraHandle, const uint8_t* source, int32_t sourceLen, int32_t padding, const EVP_MD* digest, uint8_t* destination, int32_t destinationLen);
EVP_PKEY* CryptoNative_RsaGenerateKey(int32_t keySize);
int32_t CryptoNative_RsaSignHash(EVP_PKEY* pkey, void* extraHandle, int32_t padding, const EVP_MD* digest, const uint8_t* hash, int32_t hashLen, uint8_t* destination, int32_t destinationLen);
int32_t CryptoNative_RsaVerifyHash(EVP_PKEY* pkey, void* extraHandle, int32_t padding, const EVP_MD* digest, const uint8_t* hash, int32_t hashLen, const uint8_t* signature, int32_t signatureLen);
/* pal_evp_pkey_slh_dsa.h */
int32_t CryptoNative_SlhDsaExportPublicKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength);
int32_t CryptoNative_SlhDsaExportSecretKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength);
EVP_PKEY* CryptoNative_SlhDsaGenerateKey(const char* keyType);
int32_t CryptoNative_SlhDsaGetPalId(const EVP_PKEY* pKey, int32_t* slhDsaTypeId);
int32_t CryptoNative_SlhDsaSignPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* destination, int32_t destinationLen);
int32_t CryptoNative_SlhDsaSignPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* destination, int32_t destinationLen);
int32_t CryptoNative_SlhDsaVerifyPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* sig, int32_t sigLen);
int32_t CryptoNative_SlhDsaVerifyPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* sig, int32_t sigLen);
/* pal_evp_pkey_x25519.h */
int32_t CryptoNative_X25519Available(void);
int32_t CryptoNative_X25519DeriveSecretAgreementWithBytes(EVP_PKEY* pkey, void* extraHandle, const uint8_t* peerKey, int32_t peerKeyLength, uint8_t* secret, uint32_t secretLength);
int32_t CryptoNative_X25519ExportPrivateKey(const EVP_PKEY* key, uint8_t* destination, int32_t destinationLength);
int32_t CryptoNative_X25519ExportPublicKey(const EVP_PKEY* key, uint8_t* destination, int32_t destinationLength);
EVP_PKEY* CryptoNative_X25519GenerateKey(void);
EVP_PKEY* CryptoNative_X25519ImportPrivateKey(const uint8_t* source, int32_t sourceLength);
EVP_PKEY* CryptoNative_X25519ImportPublicKey(const uint8_t* source, int32_t sourceLength);
int32_t CryptoNative_X25519IsValidHandle(const EVP_PKEY* key, int32_t* hasPrivateKey);
/* pal_hmac.h */
DN_MAC_CTX* CryptoNative_HmacCopy(const DN_MAC_CTX* ctx);
DN_MAC_CTX* CryptoNative_HmacCreate(uint8_t* key, int32_t keyLen, const EVP_MD* md);
int32_t CryptoNative_HmacCurrent(const DN_MAC_CTX* ctx, uint8_t* md, int32_t* len);
void CryptoNative_HmacDestroy(DN_MAC_CTX* ctx);
int32_t CryptoNative_HmacFinal(DN_MAC_CTX* ctx, uint8_t* md, int32_t* len);
int32_t CryptoNative_HmacOneShot(const EVP_MD* type, const uint8_t* key, int32_t keySize, const uint8_t* source, int32_t sourceSize, uint8_t* md, int32_t* mdSize);
int32_t CryptoNative_HmacReset(DN_MAC_CTX* ctx);
int32_t CryptoNative_HmacUpdate(DN_MAC_CTX* ctx, const uint8_t* data, int32_t len);
/* pal_ocsp.h */
OCSP_RESPONSE* CryptoNative_DecodeOcspResponse(const uint8_t* buf, int32_t len);
int32_t CryptoNative_EncodeOcspRequest(OCSP_REQUEST* req, uint8_t* buf);
int32_t CryptoNative_GetOcspRequestDerSize(OCSP_REQUEST* req);
void CryptoNative_OcspRequestDestroy(OCSP_REQUEST* request);
void CryptoNative_OcspResponseDestroy(OCSP_RESPONSE* response);
/* pal_pkcs7.h */
PKCS7* CryptoNative_D2IPkcs7Bio(BIO* bp);
PKCS7* CryptoNative_DecodePkcs7(const uint8_t* buf, int32_t len);
int32_t CryptoNative_EncodePkcs7(PKCS7* p7, uint8_t* buf);
int32_t CryptoNative_GetPkcs7Certificates(PKCS7* p7, X509Stack** certs);
int32_t CryptoNative_GetPkcs7DerSize(PKCS7* p7);
PKCS7* CryptoNative_PemReadBioPkcs7(BIO* bp);
PKCS7* CryptoNative_Pkcs7CreateCertificateCollection(X509Stack* certs);
void CryptoNative_Pkcs7Destroy(PKCS7* p7);
/* pal_ssl.h */
void CryptoNative_EnsureLibSslInitialized(void);
int32_t CryptoNative_GetDefaultSignatureAlgorithms(uint16_t* buffer, int32_t* count);
const char* CryptoNative_GetOpenSslCipherSuiteName(SSL* ssl, int32_t cipherSuite, int32_t* isTls12OrLower);
int32_t CryptoNative_IsSslRenegotiatePending(SSL* ssl);
int32_t CryptoNative_IsSslStateOK(SSL* ssl);
int32_t CryptoNative_OpenSslGetProtocolSupport(int32_t protocol);
int32_t CryptoNative_SslAddClientCAs(SSL* ssl, X509** x509s, uint32_t count);
int32_t CryptoNative_SslAddExtraChainCert(SSL* ssl, X509* x509);
SSL* CryptoNative_SslCreate(SSL_CTX* ctx);
int32_t CryptoNative_SslCtxAddExtraChainCert(SSL_CTX* ctx, X509* x509);
int32_t CryptoNative_SslCtxCheckPrivateKey(SSL_CTX* ctx);
SSL_CTX* CryptoNative_SslCtxCreate(const SSL_METHOD* method);
void CryptoNative_SslCtxDestroy(SSL_CTX* ctx);
void* CryptoNative_SslCtxGetData(SSL_CTX* ctx);
int CryptoNative_SslCtxRemoveSession(SSL_CTX* ctx, SSL_SESSION* session);
void CryptoNative_SslCtxSetAlpnSelectCb(SSL_CTX* ctx, SslCtxSetAlpnCallback cb, void* arg);
int CryptoNative_SslCtxSetCaching(SSL_CTX* ctx, int mode, int cacheSize, int contextIdLength, uint8_t* contextId, SslCtxNewSessionCallback newSessionCb, SslCtxRemoveSessionCallback removeSessionCb);
void CryptoNative_SslCtxSetCertVerifyCallback(SSL_CTX* ctx, SslCtxCertVerifyCallback callback);
int32_t CryptoNative_SslCtxSetCiphers(SSL_CTX* ctx, const char* cipherList, const char* cipherSuites);
int32_t CryptoNative_SslCtxSetData(SSL_CTX* ctx, void* ptr);
void CryptoNative_SslCtxSetDefaultOcspCallback(SSL_CTX* ctx);
int32_t CryptoNative_SslCtxSetEncryptionPolicy(SSL_CTX* ctx, int32_t policy);
void CryptoNative_SslCtxSetKeylogCallback(SSL_CTX* ctx, SslCtxSetKeylogCallback callback);
void CryptoNative_SslCtxSetProtocolOptions(SSL_CTX* ctx, int32_t protocols);
void CryptoNative_SslCtxSetQuietShutdown(SSL_CTX* ctx);
int32_t CryptoNative_SslCtxUseCertificate(SSL_CTX* ctx, X509* x);
int32_t CryptoNative_SslCtxUsePrivateKey(SSL_CTX* ctx, EVP_PKEY* pkey);
int32_t CryptoNative_SslDecrypt(SSL* ssl, uint8_t* inputPtr, int32_t inputLen, int32_t* consumed, uint8_t* outputPtr, int32_t outputCap, int32_t* leftoverOffset, int32_t* leftoverLength, int32_t* errorCode);
void CryptoNative_SslDestroy(SSL* ssl);
int32_t CryptoNative_SslDoHandshake(SSL* ssl, int32_t* errorCode);
int32_t CryptoNative_SslEncrypt(SSL* ssl, const uint8_t* plaintextPtr, int32_t plaintextLen, uint8_t* outputPtr, int32_t outputCap, int32_t* outputWritten, int32_t* outputPending, int32_t* errorCode);
void CryptoNative_SslGet0AlpnSelected(SSL* ssl, const uint8_t** protocol, uint32_t* len);
X509* CryptoNative_SslGetCertificate(SSL* ssl);
X509NameStack* CryptoNative_SslGetClientCAList(SSL* ssl);
int32_t CryptoNative_SslGetCurrentCipherId(SSL* ssl, int32_t* cipherId);
void* CryptoNative_SslGetData(SSL* ssl);
int32_t CryptoNative_SslGetError(SSL* ssl, int32_t ret);
int32_t CryptoNative_SslGetFinished(SSL* ssl, void* buf, int32_t count);
X509Stack* CryptoNative_SslGetPeerCertChain(SSL* ssl);
X509* CryptoNative_SslGetPeerCertificate(SSL* ssl);
int32_t CryptoNative_SslGetPeerFinished(SSL* ssl, void* buf, int32_t count);
const char* CryptoNative_SslGetServerName(SSL* ssl);
SSL_SESSION* CryptoNative_SslGetSession(SSL* ssl);
SSL_CTX* CryptoNative_SslGetSslCtx(SSL* ssl);
const char* CryptoNative_SslGetVersion(SSL* ssl);
int32_t CryptoNative_SslHandshake(SSL* ssl, const uint8_t* inputPtr, int32_t inputLen, int32_t* consumed, uint8_t* outputPtr, int32_t outputCap, int32_t* outputWritten, int32_t* outputPending, int32_t* errorCode);
int32_t CryptoNative_SslRead(SSL* ssl, void* buf, int32_t num, int32_t* error);
int32_t CryptoNative_SslRenegotiate(SSL* ssl, int32_t* error);
void CryptoNative_SslSessionFree(SSL_SESSION* session);
void* CryptoNative_SslSessionGetData(SSL_SESSION* session);
const char* CryptoNative_SslSessionGetHostname(SSL_SESSION* session);
int32_t CryptoNative_SslSessionReused(SSL* ssl);
void CryptoNative_SslSessionSetData(SSL_SESSION* session, void* val);
int CryptoNative_SslSessionSetHostname(SSL_SESSION* session, const char* hostname);
void CryptoNative_SslSetAcceptMovingWriteBuffer(SSL* ssl);
void CryptoNative_SslSetAcceptState(SSL* ssl);
int32_t CryptoNative_SslSetAlpnProtos(SSL* ssl, const uint8_t* protos, uint32_t protos_len);
void CryptoNative_SslSetBio(SSL* ssl, BIO* rbio, BIO* wbio);
void CryptoNative_SslSetClientCertCallback(SSL* ssl, int set);
int32_t CryptoNative_SslSetClientSigalgs(SSL* ssl, uint8_t* str);
void CryptoNative_SslSetConnectState(SSL* ssl);
int32_t CryptoNative_SslSetData(SSL* ssl, void* ptr);
int32_t CryptoNative_SslSetFd(SSL* ssl, intptr_t fd);
void CryptoNative_SslSetPostHandshakeAuth(SSL* ssl, int32_t val);
void CryptoNative_SslSetQuietShutdown(SSL* ssl, int mode);
int32_t CryptoNative_SslSetRetryVerify(SSL* ssl);
int32_t CryptoNative_SslSetSession(SSL* ssl, SSL_SESSION* session);
int32_t CryptoNative_SslSetSigalgs(SSL* ssl, uint8_t* str);
int32_t CryptoNative_SslSetTlsExtHostName(SSL* ssl, uint8_t* name);
void CryptoNative_SslSetVerifyPeer(SSL* ssl, int32_t failIfNoPeerCert);
int32_t CryptoNative_SslShutdown(SSL* ssl);
void CryptoNative_SslStapleOcsp(SSL* ssl, uint8_t* buf, int32_t len);
void CryptoNative_SslUpdateOcspStaple(SSL* ssl, X509* cert);
int32_t CryptoNative_SslUseCertificate(SSL* ssl, X509* x);
int32_t CryptoNative_SslUsePrivateKey(SSL* ssl, EVP_PKEY* pkey);
const SSL_METHOD* CryptoNative_SslV2_3Method(void);
int32_t CryptoNative_SslWrite(SSL* ssl, const void* buf, int32_t num, int32_t* error);
int32_t CryptoNative_Tls13Supported(void);
SSL* CryptoNative_X509StoreCtxGetSslPtr(X509_STORE_CTX* storeCtx);
void CryptoNative_X509StoreCtxSetError(X509_STORE_CTX* storeCtx, int32_t error);
/* pal_x509.h */
X509* CryptoNative_DecodeX509(const uint8_t* buf, int32_t len);
X509_CRL* CryptoNative_DecodeX509Crl(const uint8_t* buf, int32_t len);
int32_t CryptoNative_EncodeX509(X509* x, uint8_t* buf);
int32_t CryptoNative_EncodeX509SubjectPublicKeyInfo(X509* x, uint8_t* buf);
int32_t CryptoNative_GetX509DerSize(X509* x);
EVP_PKEY* CryptoNative_GetX509EvpPublicKey(X509* x509);
int32_t CryptoNative_GetX509SubjectPublicKeyInfoDerSize(X509* x);
X509_CRL* CryptoNative_PemReadBioX509Crl(BIO* bio);
X509* CryptoNative_PemReadX509FromBio(BIO* bio);
X509* CryptoNative_PemReadX509FromBioAux(BIO* bio);
int32_t CryptoNative_PemWriteBioX509Crl(BIO* bio, X509_CRL* crl);
OCSP_REQUEST* CryptoNative_X509BuildOcspRequest(X509* subject, X509* issuer);
OCSP_REQUEST* CryptoNative_X509ChainBuildOcspRequest(X509_STORE_CTX* storeCtx, int chainDepth);
int32_t CryptoNative_X509ChainGetCachedOcspStatus(X509_STORE_CTX* storeCtx, char* cachePath, int chainDepth);
int32_t CryptoNative_X509ChainHasStapledOcsp(X509_STORE_CTX* storeCtx);
X509_STORE* CryptoNative_X509ChainNew(X509Stack* systemTrust, X509Stack* userTrust);
int32_t CryptoNative_X509ChainVerifyOcsp(X509_STORE_CTX* storeCtx, OCSP_REQUEST* req, OCSP_RESPONSE* resp, char* cachePath, int chainDepth);
int32_t CryptoNative_X509CheckPurpose(X509* x, int32_t id, int32_t ca);
void CryptoNative_X509CrlDestroy(X509_CRL* a);
int32_t CryptoNative_X509DecodeOcspToExpiration(const uint8_t* buf, int32_t len, OCSP_REQUEST* req, X509* subject, X509** issuers, int issuersLen, int64_t* expiration);
void CryptoNative_X509Destroy(X509* a);
int32_t CryptoNative_X509ExtensionGetCritical(X509_EXTENSION* x);
const ASN1_OCTET_STRING* CryptoNative_X509ExtensionGetData(X509_EXTENSION* x);
const ASN1_OBJECT* CryptoNative_X509ExtensionGetOid(X509_EXTENSION* x);
const ASN1_OCTET_STRING* CryptoNative_X509FindExtensionData(X509* x, int32_t nid);
const X509_EXTENSION* CryptoNative_X509GetExt(X509* x, int32_t loc);
int32_t CryptoNative_X509GetExtCount(X509* x);
const X509_NAME* CryptoNative_X509GetIssuerName(X509* x509);
ASN1_INTEGER* CryptoNative_X509GetSerialNumber(X509* x509);
const X509_NAME* CryptoNative_X509GetSubjectName(X509* x509);
uint64_t CryptoNative_X509IssuerNameHash(X509* x);
int32_t CryptoNative_X509StackAddDirectoryStore(X509Stack* stack, char* storePath);
int32_t CryptoNative_X509StackAddMultiple(X509Stack* dest, X509Stack* src);
int32_t CryptoNative_X509StoreAddCrl(X509_STORE* ctx, X509_CRL* x);
int32_t CryptoNative_X509StoreCtxCommitToChain(X509_STORE_CTX* storeCtx);
X509_STORE_CTX* CryptoNative_X509StoreCtxCreate(void);
void CryptoNative_X509StoreCtxDestroy(X509_STORE_CTX* v);
void* CryptoNative_X509StoreCtxGetAppData(X509_STORE_CTX* ctx);
X509Stack* CryptoNative_X509StoreCtxGetChain(X509_STORE_CTX* ctx);
X509* CryptoNative_X509StoreCtxGetCurrentCert(X509_STORE_CTX* ctx);
int32_t CryptoNative_X509StoreCtxGetError(X509_STORE_CTX* ctx);
int32_t CryptoNative_X509StoreCtxGetErrorDepth(X509_STORE_CTX* ctx);
X509Stack* CryptoNative_X509StoreCtxGetSharedUntrusted(X509_STORE_CTX* ctx);
int32_t CryptoNative_X509StoreCtxInit(X509_STORE_CTX* ctx, X509_STORE* store, X509* x509, X509Stack* extraStore);
int32_t CryptoNative_X509StoreCtxRebuildChain(X509_STORE_CTX* ctx);
int32_t CryptoNative_X509StoreCtxReset(X509_STORE_CTX* ctx);
int32_t CryptoNative_X509StoreCtxResetForSignatureError(X509_STORE_CTX* storeCtx, X509_STORE** newStore);
int32_t CryptoNative_X509StoreCtxSetVerifyCallback(X509_STORE_CTX* ctx, X509StoreVerifyCallback callback, void* appData);
void CryptoNative_X509StoreDestroy(X509_STORE* v);
int32_t CryptoNative_X509StoreSetRevocationFlag(X509_STORE* ctx, int32_t revocationFlag);
X509* CryptoNative_X509UpRef(X509* x509);
int32_t CryptoNative_X509VerifyCert(X509_STORE_CTX* ctx);
const char* CryptoNative_X509VerifyCertErrorString(int32_t n);
/* pal_x509_name.h */
X509_NAME* CryptoNative_GetX509NameStackField(X509NameStack* sk, int32_t loc);
int32_t CryptoNative_GetX509NameStackFieldCount(X509NameStack* sk);
/* pal_x509_root.h */
const char* CryptoNative_GetX509RootStoreFile(uint8_t* defaultPath);
const char* CryptoNative_GetX509RootStorePath(uint8_t* defaultPath);
/* pal_x509ext.h */
X509_EXTENSION* CryptoNative_X509ExtensionCreateByObj(ASN1_OBJECT* obj, int32_t isCritical, ASN1_OCTET_STRING* data);
void CryptoNative_X509ExtensionDestroy(X509_EXTENSION* a);
int32_t CryptoNative_X509V3ExtPrint(BIO* out, X509_EXTENSION* ext);

#ifdef __cplusplus
}
#endif

#endif /* RHU_PAL_SURFACE_NET11_RC2_H */
