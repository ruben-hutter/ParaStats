import sys
from single_flight import SingleFlight


def main():
    # check num of arguments
    num_args = 1
    if num_args == 1:
        # parse data
        file = sys.argv[1]
        my_flight = SingleFlight(file)

        # print
        my_flight.print_flight_stats()


if __name__ == "__main__":
    main()
