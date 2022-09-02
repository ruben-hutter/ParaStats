import sys
from single_flight import SingleFlight
from multiple_flights import MultipleFlights


def main():
    # check num of arguments
    num_args = len(sys.argv)
    if num_args == 2:
        # parse data
        arg = sys.argv[1]
        if not arg.endswith(".igc") and not arg.endswith(".IGC"):
            # directory path
            # create multiple flights obj
            my_flights = MultipleFlights(arg)
            # print
            my_flights.print_stats()
        else:
            # single file
            # create single flight obj
            my_flight = SingleFlight(arg)
            # print
            my_flight.print_stats()
    else:
        print("Not correct number of parameters...")
        print("Run: `python launcher.py [file.igc | directory_path]`")


if __name__ == "__main__":
    main()
