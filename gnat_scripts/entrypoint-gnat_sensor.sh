#!/bin/bash


if [ ! -z "${GNAT_PCAP_LIST}" ]; then
    GNAT_INPUT_SPEC="--in ${GNAT_PCAP_LIST} --caplist"
    echo "pcap offline: ${GNAT_INPUT_SPEC}"
elif [ ! -z "${GNAT_INTERFACE}" ]; then
    GNAT_INPUT_SPEC="--in ${GNAT_INTERFACE} --live=pcap"
     echo "pcap live: ${GNAT_INPUT_SPEC}"
else
    echo "Missing environment variable GNAT_INTERFACE or GNAT_PCAP_LIST"
    exit 1
fi

if [ -z "${GNAT_EXPORT_INTERVAL}" ]; then
    GNAT_EXPORT_INTERVAL=20
fi


if [ -z "${GNAT_OPTIONS}" ]; then
    GNAT_OPTIONS="--entropy --ndpi --verbose  --mac --max-payload=8192 --flow-stats --no-tombstone --no-stats --active-timeout 60 --idle-timeout 60"
    if [ -z "${GNAT_IPFIX_HOST}" ]; then

        if [ -z "${GNAT_OUTPUT}" ]; then
            GNAT_OUTPUT=/var/spool/${GNAT_OBSERVATION_TAG}
        fi

        if [ ! -d  "${GNAT_OUTPUT}" ]; then
            mkdir -p ${GNAT_OUTPUT}
        fi

        for file in ${GNAT_OUTPUT}/*.lock; do
            if [ -f "$file" ]; then
                echo "removing $file"
                rm $file
            fi
        done
        GNAT_OPTIONS=${GNAT_OPTIONS}" --out ${GNAT_OUTPUT}/${GNAT_OBSERVATION_TAG} --lock --no-template-metadata --no-element-metadata --rotate ${GNAT_EXPORT_INTERVAL}"
        echo "exporting to local spool directory: ${GNAT_OUTPUT}"
    else
       
        GNAT_OPTIONS=${GNAT_OPTIONS}" --out ${GNAT_IPFIX_HOST}"
        
        if [ ! -z "${GNAT_IPFIX_PROTO}" ]; then
            GNAT_OPTIONS=${GNAT_OPTIONS}" --ipfix ${GNAT_IPFIX_PROTO}"
        fi

        if [ ! -z "${GNAT_IPFIX_PORT}" ]; then
            GNAT_OPTIONS=${GNAT_OPTIONS}" --ipfix-port ${GNAT_IPFIX_PORT}"
        fi
        echo "exporting to ipfix host: ${GNAT_IPFIX_HOST}:${GNAT_IPFIX_PORT} ${GNAT_IPFIX_PROTO}"
    fi
fi

export LTDL_LIBRARY_PATH=/opt/gnat/lib/yaf

if [ ! -z "${GNAT_PCAP_LIST}" ]; then
    /opt/gnat/bin/gnat_yaf ${GNAT_INPUT_SPEC} ${GNAT_OPTIONS}
    echo "finished processing pcap list: ${GNAT_PCAP_LIST}"
    # if in pcap procesing mode, then sleep until the service is explicity shut down
    while true
    do
        sleep 1
    done
else
    /opt/gnat/bin/gnat_sensor ${GNAT_INPUT_SPEC} ${GNAT_OPTIONS} 
fi
