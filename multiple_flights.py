import numpy as np
import os
from single_flight import SingleFlight


class MultipleFlights:
    def __init__(self, directory_path):
        self.flights = []
        # initialize obj
        self.initialize_flights(directory_path)
        pass

    def initialize_flights(self, directory_path):
        # iterate over directory files
        for filename in os.listdir(directory_path):
            # create single flight obj for every file
            self.flights.append(SingleFlight(os.path.join(directory_path, filename)))

    def print_stats(self):
        for flight in self.flights:
            flight.print_stats()
