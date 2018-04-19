AM_CFLAGS =-std=gnu99 --pedantic -Wall -Werror -g -O2 -I$(top_builddir)/include

lib_LTLIBRARIES=libcjose.la
libcjose_la_CPPFLAGS= -I$(topdir)/include
libcjose_la_LDFLAGS= -lm
libcjose_la_SOURCES=version.c \
					util.c \
					base64.c \
					concatkdf.c \
                    jwk.c \
                    jwe.c \
                    jws.c \
                    header.c \
                    error.c \
					include/concatkdf_int.h \
					include/header_int.h \
					include/jwk_int.h \
					include/jwe_int.h \
					include/jws_int.h \
					include/util_int.h
