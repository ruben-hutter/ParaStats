import sys
import os
from single_flight import SingleFlight


def main():
    # check num of arguments
    num_args = len(sys.argv)
    if num_args == 1:
        flights = []
        directory = "./Data/"
        # iterate over directory files
        for filename in os.listdir(directory):
            # create single flight obj for every file
            flights.append(SingleFlight(os.path.join(directory, filename)))
        for flight in flights:
            flight.print_flight_stats()
    elif num_args == 2:
        # parse data
        file = sys.argv[1]
        # create single flight obj
        my_flight = SingleFlight(file)
        # print
        my_flight.print_flight_stats()
    else:
        print("Not correct number of parameters...")


if __name__ == "__main__":
    main()
