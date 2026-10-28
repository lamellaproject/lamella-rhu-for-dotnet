/* The one name-resolution call the stock .NET serial layer's error-string
 * helper reaches for: the common errno header includes
 * netdb.h for the resolver's error names and guards every use of them with
 * #ifdef, so none is defined here and those branches vanish; the string
 * call's prototype stays for the one line that names it. The kit's C
 * library has no netdb.h. */
#ifndef RHU_PORTS_NETDB_H
#define RHU_PORTS_NETDB_H

const char *gai_strerror(int code);

#endif /* RHU_PORTS_NETDB_H */
