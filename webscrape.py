from time import sleep
from selenium import webdriver
from selenium.webdriver.common.keys import Keys
from selenium.webdriver.common.by import By
import zipfile
import os
from datetime import date
import time

# max time for download
TIMEOUT = 20
download_path = os.path.join(os.environ['HOME'], "Downloads/")
url = "https://www.xcontest.org/world/en/"
driver = webdriver.Firefox()


def get_user_data():
    print("===== Enter login data =====")
    username = input("Username: ")
    password = input("Password: ")
    return username, password


def login(username, password):
    login_username = driver.find_element(By.ID, "login-username")
    login_username.send_keys(username)
    login_password = driver.find_element(By.ID, "login-password")
    login_password.send_keys(password)
    login_password.send_keys(Keys.ENTER)


def download_flights(file_path):
    print("===== Downloading... =====")
    initial_number = check_number_of_files(download_path)
    download_igc = driver.find_element(By.LINK_TEXT, "IGC")
    driver.execute_script("arguments[0].click();", download_igc)
    # wait until download completed
    # 1) firefox creates the end file and a tmp file for the download
    # 2) count number of files before download
    # 3) check until only 1 new file instead of 2 -> download finished
    dl_completed = False
    t_start = time.time()
    t_elapsed = 0
    while not dl_completed and t_elapsed < TIMEOUT:
        sleep(1)
        t_elapsed = time.time() - t_start
        # check if file exists
        if os.path.isfile(file_path) and check_number_of_files(download_path) == initial_number + 1:
            print("===== Finished download =====")
            return True
    print("===== Error downloading flights! Try again later! =====")
    return False


def check_number_of_files(path):
    directory = os.listdir(path)
    number_of_files = len(directory)
    return number_of_files


def extract_flights(file_path, username):
    print("===== Extracting... =====")
    with zipfile.ZipFile(file_path, 'r') as zip_ref:
        zip_ref.extractall(f"{download_path}/{username}_flights")
    print("===== Finished =====")


def remove_temp_files():
    for file in os.listdir(download_path):
        # TODO check if it works
        if os.path.isfile(file) and file.endswith(".zip.part"):
            os.remove(file)


def main():
    # get user's xcontest credentials
    username, password = get_user_data()
    file_name = f"igc.{username}.world{str(date.today().year)}.zip"
    file_path = download_path + file_name
    # open webpage
    driver.get(url)
    # wait to load page
    driver.implicitly_wait(3)
    # login with credentials
    login(username, password)
    # go to my_flights
    my_flights = driver.find_element(By.LINK_TEXT, "My flights")
    my_flights.click()
    # download all flights
    download_ok = False
    choice = "r"
    while not download_ok and choice == "r":
        download_ok = download_flights(file_path)
        # check if flights downloaded correctly
        if download_ok:
            # extract flights
            extract_flights(file_path, username)
        else:
            print("===== Download failed =====")
            choice = input("[R]etry | [C]ancel: ").lower()
    # remove possible temp files
    remove_temp_files()
    driver.quit()


if __name__ == "__main__":
    main()
