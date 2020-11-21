from aws_kinesis_agg.kpl_pb2 import AggregatedRecord
from aws_kinesis_agg import MAGIC, DIGEST_SIZE

import argparse
import base64
import hashlib

if __name__ == '__main__':
    parser = argparse.ArgumentParser(
        description="Decodes a base64 encoded kinesis record")
    parser.add_argument('--input', required=True, help='input data')
    args = parser.parse_args()

    decoded = base64.b64decode(args.input)
    magic = decoded[:len(MAGIC)]
    message_data = decoded[len(MAGIC):-DIGEST_SIZE]
    checksum = decoded[-DIGEST_SIZE:]

    md5_calc = hashlib.md5()
    md5_calc.update(message_data)
    expected_checksum = md5_calc.digest()

    assert magic == MAGIC
    assert checksum == expected_checksum

    ar = AggregatedRecord()
    ar.ParseFromString(message_data)

    print(ar)
