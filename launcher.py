import sys
from single_flight import SingleFlight
from multiple_flights import MultipleFlights


def main():
    # check num of arguments
    num_args = len(sys.argv)
    if num_args == 1:
        directory_path = "./Data/"
        # create multiple flights obj
        my_flights = MultipleFlights(directory_path)
        # print
        my_flights.print_stats()
    elif num_args == 2:
        # parse data
        file = sys.argv[1]
        # create single flight obj
        my_flight = SingleFlight(file)
        # print
        my_flight.print_stats()
    else:
        print("Not correct number of parameters...")


if __name__ == "__main__":
    main()
